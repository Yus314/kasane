{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
    git-hooks = {
      url = "github:cachix/git-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { nixpkgs, rust-overlay, flake-utils, git-hooks, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rustfmt" "clippy" ];
          targets = [ "wasm32-unknown-unknown" "wasm32-wasip2" ];
        };
        isLinux = pkgs.stdenv.isLinux;

        # Common GUI dependencies (Linux only)
        guiBuildInputs = pkgs.lib.optionals isLinux [
          pkgs.vulkan-loader
          pkgs.wayland
          pkgs.wayland-protocols
          pkgs.libxkbcommon
          pkgs.libX11
          pkgs.libXcursor
          pkgs.libXrandr
          pkgs.libXi
          pkgs.fontconfig
          pkgs.freetype
        ];

        guiRuntimeLibs = pkgs.lib.optionals isLinux [
          pkgs.vulkan-loader
          pkgs.wayland
          pkgs.libxkbcommon
        ];

        mkKasane = { withGui ? false }: pkgs.rustPlatform.buildRustPackage {
          pname = "kasane";
          version = "0.3.0";

          src = lib.cleanSourceWith {
            src = ./.;
            filter = path: type:
              let
                relPath = lib.removePrefix (toString ./. + "/") (toString path);
                isExcluded =
                  lib.hasPrefix "docs/" relPath
                  || lib.hasPrefix "examples/" relPath
                  || lib.hasPrefix "contrib/" relPath
                  || lib.hasPrefix "tools/" relPath
                  || lib.hasPrefix ".github/" relPath
                  || lib.hasPrefix ".claude/" relPath
                  || relPath == "CLAUDE.md";
              in
              !isExcluded;
          };

          cargoLock.lockFile = ./Cargo.lock;

          cargoBuildFlags = [ "-p" "kasane" ]
            ++ lib.optionals withGui [ "--features" "gui" ];

          # Skip tests that require kakoune or a TTY
          doCheck = true;
          cargoTestFlags = [ "-p" "kasane-core" "-p" "kasane-tui" ];

          nativeBuildInputs = [
            pkgs.pkg-config
            pkgs.makeWrapper
          ];

          buildInputs = lib.optionals withGui guiBuildInputs;

          postInstall = let
            wrapArgs = lib.concatStringsSep " " ([
              "--prefix PATH : ${lib.makeBinPath [ pkgs.kakoune ]}"
            ] ++ lib.optionals (withGui && isLinux) [
              "--prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath guiRuntimeLibs}"
            ]);
          in ''
            wrapProgram $out/bin/kasane ${wrapArgs}
          '';

          meta = with lib; {
            description = "Alternative frontend for the Kakoune text editor";
            homepage = "https://github.com/Yus314/kasane";
            license = with licenses; [ mit asl20 ];
            platforms = platforms.linux ++ platforms.darwin;
            mainProgram = "kasane";
          };
        };

        lib = pkgs.lib;

        pre-commit-check = git-hooks.lib.${system}.run {
          src = ./.;
          hooks = {
            rustfmt = {
              enable = true;
              packageOverrides = {
                cargo = rustToolchain;
                rustfmt = rustToolchain;
              };
            };
            doc-consistency = {
              enable = true;
              entry = "./tools/check-doc-consistency.sh --quick";
              language = "system";
              pass_filenames = false;
              files = "(plugin\\.wit|config\\.rs|docs/.*\\.md|README\\.md)$";
              stages = [ "pre-commit" ];
            };
          };
        };
      in
      {
        packages = {
          default = mkKasane { };
          kasane = mkKasane { };
          kasane-gui = mkKasane { withGui = true; };
        };

        checks = { inherit pre-commit-check; };

        devShells.default = pkgs.mkShell {
          inherit (pre-commit-check) shellHook;

          buildInputs = [
            rustToolchain
            pkgs.pkg-config
          ] ++ pkgs.lib.optionals isLinux [
            # GUI backend dependencies (Linux)
            pkgs.vulkan-loader
            pkgs.wayland
            pkgs.wayland-protocols
            pkgs.libxkbcommon
            pkgs.libX11
            pkgs.libXcursor
            pkgs.libXrandr
            pkgs.libXi
            pkgs.fontconfig
            pkgs.freetype
          ];

          LD_LIBRARY_PATH = pkgs.lib.optionalString isLinux (pkgs.lib.makeLibraryPath [
            pkgs.vulkan-loader
            pkgs.wayland
            pkgs.libxkbcommon
          ]);
        };
      }
    );
}
