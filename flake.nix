{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    treefmt-nix.url = "github:numtide/treefmt-nix";
  };

  outputs =
    inputs:
    inputs.flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "x86_64-linux"
        "aarch64-darwin"
      ];

      imports = [
        inputs.treefmt-nix.flakeModule
      ];

      perSystem =
        {
          system,
          ...
        }:
        let
          overlay =
            final: prev:
            let
              rustPlatform = prev.rustPlatform;
            in
            {
              inherit rustPlatform;
            };
          pkgs = import inputs.nixpkgs {
            inherit system;
            overlays = [ overlay ];
          };
        in
        {
          treefmt = {
            programs.nixfmt.enable = true;
            programs.rustfmt.enable = true;
          };

          devShells.default = pkgs.mkShell {
            packages = with pkgs; [
              cargo
              rustc
              rust-analyzer
              clippy
            ];
          };
        };
    };
}
