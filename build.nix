let
	pkgs = import <nixpkgs> {};
in {
  libh264bitstream = (pkgs.callPackage ./h264bitstream.nix {});
}
