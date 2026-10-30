{
  description = "Lattice: the keyboard hacker's audio tracker";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { nixpkgs, ... }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in {
      devShells = forAll (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [ cargo rustc rustfmt clippy rust-analyzer pkg-config ];
          # cpal (ALSA, with PipeWire's ALSA plugin) and midir both link against alsa-lib.
          buildInputs = with pkgs; [ alsa-lib ];
        };
      });
    };
}
