{
  lib,
  stdenv,
  rustPlatform,
  fetchFromGitHub,
  pkg-config,
  kakoune,
  makeBinaryWrapper,
  versionCheckHook,
  nix-update-script,
  withGui ? true,
  vulkan-loader,
  wayland,
  libxkbcommon,
  libx11,
  libxcursor,
  libxrandr,
  libxi,
  fontconfig,
  freetype,
}:

rustPlatform.buildRustPackage (finalAttrs: {
  __structuredAttrs = true;

  pname = "kasane";
  version = "0.6.0";

  src = fetchFromGitHub {
    owner = "Yus314";
    repo = "kasane";
    tag = "v${finalAttrs.version}";
    hash = "sha256-1RyZfIRLviW3PTfQn7uTpt6OEtgrbZu6LFFIUVC+Wwc=";
  };

  cargoHash = "sha256-WQEpg5+AxpgsldKcYkfMEx0SSXHfnU6H1pXXiXPLQN8=";

  cargoBuildFlags = [
    "-p"
    "kasane"
  ];
  buildFeatures = lib.optional withGui "gui";

  # gui feature only exists in the kasane crate, not in test targets
  checkFeatures = [ ];

  # kasane crate tests require a running kakoune process
  cargoTestFlags = [
    "-p"
    "kasane-core"
    "-p"
    "kasane-tui"
  ];

  nativeBuildInputs = [
    pkg-config
    makeBinaryWrapper
  ];

  buildInputs = lib.optionals (withGui && stdenv.hostPlatform.isLinux) [
    vulkan-loader
    wayland
    libxkbcommon
    libx11
    libxcursor
    libxrandr
    libxi
    fontconfig
    freetype
  ];

  # `--suffix` (not `--prefix`): if the user already has `kak` on PATH (e.g.
  # a wrapKakoune carrying their plugins + KAKOUNE_RUNTIME via home-manager
  # `programs.kakoune`), it wins. The bundled `kakoune` is only the fallback
  # for users who haven't installed Kakoune separately. Prepending would
  # replace the user's plugin-aware runtime with a plugin-less unwrapped
  # binary, which breaks any kakrc that references plugin-declared options
  # (e.g. `autothemes_dark_theme`). The required-version floor is enforced
  # at startup by kasane's own `verify_kak_version()`, so a too-old user kak
  # fails fast with an actionable message rather than silently corrupting
  # protocol parsing. This matches kasane's primary compatibility constraint
  # (existing kakrc / autoload / plugins must keep working).
  postInstall = ''
    wrapProgram $out/bin/kasane \
      --suffix PATH : ${lib.makeBinPath [ kakoune ]}
  '';

  # Vulkan, Wayland, and libxkbcommon are loaded via dlopen at runtime.
  # Add their paths after shrink-rpath so they are preserved.
  postFixup = lib.optionalString (withGui && stdenv.hostPlatform.isLinux) ''
    patchelf --add-rpath ${
      lib.makeLibraryPath [
        vulkan-loader
        wayland
        libxkbcommon
      ]
    } $out/bin/.kasane-wrapped
  '';

  nativeInstallCheckInputs = [ versionCheckHook ];
  doInstallCheck = true;

  passthru.updateScript = nix-update-script { };

  meta = {
    description = "Alternative frontend for the Kakoune text editor";
    homepage = "https://github.com/Yus314/kasane";
    license = with lib.licenses; [
      mit # OR
      asl20
    ];
    changelog = "https://github.com/Yus314/kasane/releases/tag/v${finalAttrs.version}";
    platforms = lib.platforms.linux ++ lib.platforms.darwin;
    maintainers = with lib.maintainers; [ yus314 ];
    mainProgram = "kasane";
  };
})
