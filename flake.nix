{
  description = "Lattice: the keyboard hacker's audio tracker";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { nixpkgs, ... }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in {
      # `nix run github:mochreach/lattice`. Built for any CPU of its kind rather than the one
      # building it, as .cargo/config.toml asks, since Nix may build it on another machine.
      packages = forAll (pkgs: {
        default = pkgs.rustPlatform.buildRustPackage {
          pname = "lattice";
          version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).workspace.package.version;
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = [ "--package" "lattice-tui" ];
          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.alsa-lib ];
          # The tests are run by CI; some want a sound device, which the build sandbox hasn't.
          doCheck = false;
          RUSTFLAGS = "";
          meta.mainProgram = "lattice";
        };
      });

      devShells = forAll (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [ cargo rustc rustfmt clippy rust-analyzer pkg-config ];
          # cpal (ALSA, with PipeWire's ALSA plugin) and midir both link against alsa-lib on
          # Linux; a Mac has Core Audio and Core MIDI of its own.
          buildInputs = pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.alsa-lib ];
        };
      });
    };
}
