//! Puts the commit Lattice is built from into `lattice --version`, when it's built from a git
//! checkout; a build from anywhere else goes without.

use std::process::Command;

fn main() {
    let commit = Command::new("git")
        .args(["describe", "--always", "--exclude", "*"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .unwrap_or_default();
    println!("cargo:rustc-env=LATTICE_COMMIT={}", commit.trim());
    // Rebuilt when the commit changes, rather than on every build.
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs");
}
