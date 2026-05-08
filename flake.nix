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
    # Kasane's JSON-RPC parser requires Kakoune ≥ v2026.04.12 (the draw-with-cursor
    # protocol — see ADR "Kakoune version: Latest stable only" and the OldProtocol
    # error in kasane-core/src/protocol/parse.rs). nixpkgs may lag the latest
    # tagged stable, so we pin the upstream source ourselves and override
    # kakoune-unwrapped instead of relying on whatever pkgs.kakoune happens to ship.
    kakoune-src = {
      url = "github:mawww/kakoune/717c665e0a1796ce04c9d4518471991f6f83d375";
      flake = false;
    };
  };

  outputs = { nixpkgs, rust-overlay, flake-utils, git-hooks, kakoune-src, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        # Override nixpkgs' kakoune-unwrapped with the pinned upstream src so the
        # PATH wrapper guarantees a Kakoune new enough for Kasane's protocol parser.
        kakouneLatest = pkgs.kakoune-unwrapped.overrideAttrs (_old: {
          version = "2026.04.12-unstable-2026-05-07";
          src = kakoune-src;
        });
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rustfmt" "clippy" ];
          targets = [ "wasm32-unknown-unknown" "wasm32-wasip2" ];
        };
        isLinux = pkgs.stdenv.isLinux;

        # Single source of truth for the kasane version: the kasane bin
        # crate's `Cargo.toml`. Auto-syncing here means a release bump
        # touches only the Rust crates (Cargo.toml + Cargo.lock) and the
        # flake follows mechanically — no separate `version = "x.y.z"`
        # field to forget. (`contrib/nixpkgs/package.nix` is excluded from
        # this scheme on purpose: it builds from a `fetchFromGitHub`
        # tarball with no working tree to read, so it is bumped manually as
        # part of the contrib-to-nixpkgs flow.)
        #
        # If a future release moves the kasane bin crate to
        # `version.workspace = true`, replace the read below with the
        # workspace Cargo.toml's `[workspace.package].version`.
        kasaneVersion =
          (builtins.fromTOML (builtins.readFile ./kasane/Cargo.toml)).package.version;

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

        # `kakoune` is an override hook: by default kasane bundles the
        # version-pinned `kakouneLatest` as a PATH fallback, but downstream
        # consumers (e.g. home-manager users with `wrapKakoune` + plugins) can
        # pass their own derivation so that kasane defers to the same Kakoune
        # they invoke directly. Compatibility with the user's kakrc / autoload
        # / plugins is the primary constraint (ADR-004A); see also the
        # `--suffix` rationale below.
        mkKasane = { withGui ? true, kakoune ? kakouneLatest }: pkgs.rustPlatform.buildRustPackage {
          pname = "kasane";
          version = kasaneVersion;

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
            # `--suffix` (not `--prefix`): if the user already has `kak` on
            # PATH (e.g. a home-manager wrapKakoune carrying their plugins +
            # KAKOUNE_RUNTIME), it wins. Our bundled kakoune is only the
            # fallback for stock installs. Prepending would replace the
            # user's plugin-aware runtime with a plugin-less unwrapped
            # binary, which breaks any kakrc that references plugin-declared
            # options (autothemes_dark_theme, etc.) — see ADR-004A. The
            # version floor is still enforced by `verify_kak_version()` at
            # startup, so a too-old user kak fails fast with an actionable
            # message rather than silently corrupting protocol parsing.
            wrapArgs = lib.concatStringsSep " " ([
              "--suffix PATH : ${lib.makeBinPath [ kakoune ]}"
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
          kasane-tui = mkKasane { withGui = false; };
        };

        checks = { inherit pre-commit-check; };

        devShells.default = pkgs.mkShell {
          inherit (pre-commit-check) shellHook;

          buildInputs = [
            rustToolchain
            pkgs.pkg-config
            kakouneLatest
          ] ++ pkgs.lib.optionals isLinux [
            pkgs.valgrind

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
