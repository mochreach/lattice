//! Sound files for samplers: read from WAV files, off the audio thread, and kept so every sampler
//! playing a file shares one copy. A file changed on disk is read again.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use lattice_core::{InstKind, Instrument};
use lattice_engine::Sample;

/// Files read so far, with when they were last changed.
type Cache = HashMap<PathBuf, (Option<SystemTime>, Arc<Sample>)>;
static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

/// What `instrument` plays, if it's a sampler with a file: `Err` if the file can't be read.
pub fn for_instrument(instrument: &Instrument) -> Result<Option<Arc<Sample>>, String> {
    match (&instrument.kind, &instrument.file) {
        (InstKind::Sampler | InstKind::Chop, Some(path)) => load(path).map(Some),
        _ => Ok(None),
    }
}

/// The file at `path` as last read, without checking whether it's changed since.
pub fn cached(path: &Path) -> Option<Arc<Sample>> {
    let cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    Some(cache.as_ref()?.get(path)?.1.clone())
}

/// Keeps what was read from `from` as what's in `to`, a copy of it.
pub fn copied(from: &Path, to: &Path) {
    let modified = std::fs::metadata(to).and_then(|m| m.modified()).ok();
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let cache = cache.get_or_insert_with(HashMap::new);
    if let Some((_, sample)) = cache.get(from) {
        let sample = sample.clone();
        cache.insert(to.to_path_buf(), (modified, sample));
    }
}

/// The audio in the WAV file at `path`.
pub fn load(path: &Path) -> Result<Arc<Sample>, String> {
    let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let cache = cache.get_or_insert_with(HashMap::new);
    if let Some((when, sample)) = cache.get(path)
        && *when == modified
    {
        return Ok(sample.clone());
    }
    let bytes = std::fs::read(path).map_err(|e| format!("can't read {}: {e}", path.display()))?;
    let sample = Arc::new(read_wav(&bytes).map_err(|e| format!("{}: {e}", path.display()))?);
    cache.insert(path.to_path_buf(), (modified, sample.clone()));
    Ok(sample)
}

/// The `N` bytes at `i` in `b`, if it has that many there.
fn bytes_at<const N: usize>(b: &[u8], i: usize) -> Option<[u8; N]> {
    b.get(i..)?.first_chunk().copied()
}

/// Reads a WAV file: integer PCM of 8, 16, 24 or 32 bits, or 32- or 64-bit float, in any number
/// of channels (the first two are kept; mono plays in both). The file could be anything, so every
/// read is checked, and a short or odd one is an `Err`.
pub fn read_wav(bytes: &[u8]) -> Result<Sample, String> {
    if bytes.get(..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return Err("not a WAV file".into());
    }
    let mut format = None;
    let mut data = None;
    let mut at: usize = 12;
    while let Some(id) = bytes.get(at..at.saturating_add(4))
        && let Some(len) = bytes_at(bytes, at.saturating_add(4))
    {
        let len = u32::from_le_bytes(len) as usize;
        let start = at.saturating_add(8);
        // The length was read, so `start` is in the file. A chunk claiming more than the file
        // holds gets what's there.
        let rest = bytes.get(start..).unwrap_or_default();
        let body = rest.get(..len).unwrap_or(rest);
        match id {
            b"fmt " if body.len() >= 16 => format = Some(body),
            b"data" => data = Some(body),
            _ => {}
        }
        // Chunks are padded to an even length.
        at = start.saturating_add(len).saturating_add(len & 1);
    }
    let format = format.ok_or("no format chunk")?;
    let data = data.ok_or("no audio in it")?;
    let u16_at = |i| bytes_at(format, i).map(u16::from_le_bytes);
    let short = "format chunk too short";
    let mut tag = u16_at(0).ok_or(short)?;
    let channels = usize::from(u16_at(2).ok_or(short)?);
    let rate = u32::from_le_bytes(bytes_at(format, 4).ok_or(short)?);
    let bits = u16_at(14).ok_or(short)?;
    if tag == 0xFFFE
        && let Some(sub) = u16_at(24)
    {
        // WAVE_FORMAT_EXTENSIBLE: the real format is the first two bytes of the subformat.
        tag = sub;
    }
    if channels == 0 || rate == 0 {
        return Err("no channels, or no sample rate".into());
    }
    let width = usize::from(bits / 8);
    let decode: fn(&[u8]) -> Option<f32> = match (tag, bits) {
        (1, 8) => |b| Some((f32::from(*b.first()?) - 128.0) / 128.0),
        (1, 16) => |b| Some(f32::from(i16::from_le_bytes(bytes_at(b, 0)?)) / 32_768.0),
        (1, 24) => |b| {
            let [x, y, z] = bytes_at(b, 0)?;
            Some(i32::from_le_bytes([0, x, y, z]) as f32 / 2_147_483_648.0)
        },
        (1, 32) => |b| Some(i32::from_le_bytes(bytes_at(b, 0)?) as f32 / 2_147_483_648.0),
        (3, 32) => |b| Some(f32::from_le_bytes(bytes_at(b, 0)?)),
        (3, 64) => |b| Some(f64::from_le_bytes(bytes_at(b, 0)?) as f32),
        _ => return Err(format!("can't read {bits}-bit audio of format {tag}")),
    };
    // Every format read is at least a byte wide, and there's at least one channel.
    let frame = width
        .checked_mul(channels)
        .filter(|&f| f > 0)
        .ok_or("bad frame size")?;
    let (mut left, mut right) = (vec![], vec![]);
    for f in data.chunks_exact(frame) {
        let mut samples = f.chunks_exact(width);
        let l = samples.next().and_then(decode).ok_or("bad sample")?;
        let r = match samples.next() {
            Some(s) => decode(s).ok_or("bad sample")?,
            None => l,
        };
        left.push(l);
        right.push(r);
    }
    if left.is_empty() {
        return Err("no audio in it".into());
    }
    Ok(Sample {
        rate: rate as f32,
        left,
        right,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A WAV file of `frames`, each `channels` samples of `bits`, in format `tag`.
    fn wav(tag: u16, channels: u16, bits: u16, data: &[u8]) -> Vec<u8> {
        let mut w = b"RIFF\0\0\0\0WAVE".to_vec();
        w.extend(b"fmt ");
        w.extend(16u32.to_le_bytes());
        w.extend(tag.to_le_bytes());
        w.extend(channels.to_le_bytes());
        w.extend(44_100u32.to_le_bytes());
        w.extend(0u32.to_le_bytes());
        w.extend((channels * bits / 8).to_le_bytes());
        w.extend(bits.to_le_bytes());
        // Something else first, of an odd length, to skip over.
        w.extend(b"LIST");
        w.extend(3u32.to_le_bytes());
        w.extend([1, 2, 3, 0]);
        w.extend(b"data");
        w.extend((data.len() as u32).to_le_bytes());
        w.extend(data);
        w
    }

    #[test]
    fn reads_mono_16_bit() {
        let data: Vec<u8> = [0i16, 16_384, -32_768]
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        let s = read_wav(&wav(1, 1, 16, &data)).unwrap();
        assert_eq!(s.rate, 44_100.0);
        assert_eq!(s.left, [0.0, 0.5, -1.0]);
        assert_eq!(s.right, s.left);
    }

    #[test]
    fn reads_stereo_float_and_24_bit() {
        let data: Vec<u8> = [0.25f32, -0.5]
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        let s = read_wav(&wav(3, 2, 32, &data)).unwrap();
        assert_eq!((s.left[0], s.right[0]), (0.25, -0.5));

        let half = 4_194_304i32.to_le_bytes();
        let s = read_wav(&wav(1, 1, 24, &half[..3])).unwrap();
        assert_eq!(s.left, [0.5]);
    }

    #[test]
    fn reads_back_an_export() {
        let path = std::env::temp_dir().join(format!("lattice-sample-{}.wav", std::process::id()));
        crate::export::write_wav(&path, &[0.5, -0.25, 0.0, 0.0], 48_000).unwrap();
        let s = load(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(s.rate, 48_000.0);
        assert!((s.left[0] - 0.5).abs() < 1e-6 && (s.right[0] + 0.25).abs() < 1e-6);
    }

    #[test]
    fn says_what_is_wrong() {
        assert_eq!(read_wav(b"hello").unwrap_err(), "not a WAV file");
        assert!(
            read_wav(&wav(1, 1, 12, &[0, 0]))
                .unwrap_err()
                .contains("12-bit")
        );
        // Cut short anywhere, or with a chunk claiming to be huge, it's an error, not a panic.
        let good = wav(3, 2, 64, &[0; 16]);
        for n in 0..good.len() {
            let _ = read_wav(&good[..n]);
        }
        let mut huge = good.clone();
        huge[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(read_wav(&huge).is_err());
        let mut short_fmt = good;
        short_fmt[16..20].copy_from_slice(&4u32.to_le_bytes());
        assert!(read_wav(&short_fmt).is_err());
        assert!(
            load(Path::new("/nowhere/at/all.wav"))
                .unwrap_err()
                .contains("can't read")
        );
    }
}
