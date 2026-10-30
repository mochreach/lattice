//! Flush denormal floats to zero on the current thread.
//!
//! As sounds decay, filter and envelope state shrinks towards zero and can become denormal,
//! which is 10–100× slower on many CPUs and causes sudden CPU spikes. Treating denormals as zero
//! makes no audible difference.

/// Sets flush-to-zero (and, on x86, denormals-are-zero) for the calling thread. Cheap enough to
/// call at the start of every audio callback.
#[inline]
pub fn flush_to_zero() {
    #[cfg(target_arch = "x86_64")]
    // SAFETY: only sets the FTZ (bit 15) and DAZ (bit 6) flags in MXCSR, which affect float
    // rounding of tiny values and nothing else.
    unsafe {
        let mut csr: u32 = 0;
        std::arch::asm!("stmxcsr [{}]", in(reg) &mut csr, options(nostack, preserves_flags));
        csr |= 0x8040;
        std::arch::asm!("ldmxcsr [{}]", in(reg) &csr, options(nostack, readonly, preserves_flags));
    }

    #[cfg(target_arch = "aarch64")]
    // SAFETY: only sets the FZ flag (bit 24) in FPCR, which affects float rounding of tiny
    // values and nothing else.
    unsafe {
        let mut fpcr: u64;
        std::arch::asm!("mrs {}, fpcr", out(reg) fpcr, options(nomem, nostack, preserves_flags));
        fpcr |= 1 << 24;
        std::arch::asm!("msr fpcr, {}", in(reg) fpcr, options(nomem, nostack, preserves_flags));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
    fn denormals_become_zero() {
        // Its own thread, so the flag doesn't leak into other tests.
        let result = std::thread::spawn(|| {
            super::flush_to_zero();
            std::hint::black_box(f32::MIN_POSITIVE) / std::hint::black_box(4.0)
        })
        .join()
        .unwrap();
        assert_eq!(result, 0.0);
    }
}
