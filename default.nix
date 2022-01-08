let
  pkgs = import <nixpkgs> {};
in pkgs.mkShell rec {
  buildInputs = [
    pkgs.automake
    pkgs.autoconf
    pkgs.libtool
    pkgs.llvmPackages.clang
  ];
  LIBCLANG_PATH = "${pkgs.llvmPackages.libclang}/lib";
}
