let
  pkgs = import <nixpkgs> {};
  pypkgs = pkgs.python39Packages;
  aubio-beat-osc = pypkgs.buildPythonPackage rec {
    pname = "aubio-beat-osc";
    version = "2019-02-19";
    src = pkgs.fetchFromGitHub {
      owner = "DrLuke";
      repo = "aubio-beat-osc";
      rev = "4c7f7ce8bc91c5036f4ded2f6800fc400e97a852";
      sha256 = "sha256-nT03ituQZfFLlWdbkbieDmpNBIMMliOCi6ebc8tkynQ=";
    };
    doCheck = false;
    propagatedBuildInputs = [
      pypkgs.pyaudio
      pypkgs.aubio
      pypkgs.python-osc
      pypkgs.numpy
    ];
  };

  open-stage-control = pkgs.stdenv.mkDerivation rec {
    pname = "open-stage-control";
    version = "1.30.2";

    src = pkgs.fetchzip {
      url = "https://openstagecontrol.ammd.net/packages/open-stage-control_${version}_linux-x64.zip";
      hash = "sha256-FMZTILqRy/k9wJlz+857ftIxhxW0qMnLlcwiLDeHmhM=";
    };

    nativeBuildInputs = [ pkgs.makeWrapper ];

    installPhase = ''
      mkdir -p $out/bin $out/lib/open-stage-control
      cp -r . $out/lib/open-stage-control/
      makeWrapper ${pkgs.electron}/bin/electron $out/bin/open-stage-control \
        --add-flags $out/lib/open-stage-control/resources/app
    '';
  };

in pkgs.mkShell rec {
  buildInputs = [
    #pkgs.llvmPackages.clang
    open-stage-control
    #aubio-beat-osc
    #pypkgs.doit

    pkgs.pkg-config
    pkgs.clang
    pkgs.llvmPackages.libclang

    # For player:
    pkgs.ffmpeg_7-headless
    pkgs.ffmpeg_7-headless.dev
    pkgs.libGL
    pkgs.libGLU
    pkgs.egl-wayland
    pkgs.xorg.libX11
    pkgs.xorg.libXcursor
    pkgs.xorg.libXrandr
    pkgs.xorg.libXi
    pkgs.xorg.libxcb
    pkgs.libxkbcommon

    # For push 2
    pkgs.alsa-lib
    pkgs.libusb1
  ];
  #LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
  #LD_PRELOAD = "${pkgs.xorg.libX11}/lib/libX11.so ${pkgs.xorg.libXcursor}/lib/libXcursor.so ${pkgs.xorg.libXrandr}/lib/libXrandr.so ${pkgs.xorg.libXi}/lib/libXi.so ${pkgs.xorg.libxcb}/lib/libxcb.so";
  shellHook = ''
    export LIBCLANG_PATH="${pkgs.llvmPackages.libclang.lib}/lib"
    export FFMPEG_DIR="${pkgs.ffmpeg_7-headless.dev}"
    export BINDGEN_EXTRA_CLANG_ARGS="-I${pkgs.ffmpeg_7-headless.dev}/include"
    export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath [
      pkgs.libGL
      pkgs.ffmpeg_7-headless.lib
      pkgs.xorg.libX11
      pkgs.xorg.libXcursor
      pkgs.xorg.libXrandr
      pkgs.xorg.libXi
      pkgs.xorg.libxcb
      pkgs.libxkbcommon
    ]}:$LD_LIBRARY_PATH"
  '';

}
