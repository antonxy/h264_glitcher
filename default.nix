let
	pkgs = import <nixpkgs> {};
in pkgs.mkShell rec {
	buildInputs = [
    pkgs.llvmPackages.clang
    (pkgs.callPackage ./h264bitstream.nix {})
	];
  LIBCLANG_PATH = "${pkgs.llvmPackages.libclang}/lib";
}
