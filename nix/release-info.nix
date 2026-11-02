{
  version = "0.0.0";

  # Stable channel pointer: SHA256 SRI hashes of each prebuilt artifact
  # published in the matching GitHub Release. ReqFlux has no tagged release
  # yet, so `artifacts` is empty and `packages.reqflux-bin` does not exist;
  # the default package falls back to the source build. The rolling nightly
  # channel is separate (see nix/nightly-info.nix).
  #
  # To point at a release:
  #
  #   ver=X.Y.Z[-rc.N]
  #   curl -fsSL -o /tmp/reqflux-amd64.tar.gz \
  #     "https://github.com/0xErwin1/reqflux/releases/download/v$ver/reqflux-linux-amd64.tar.gz"
  #   nix-hash --to-sri --type sha256 \
  #     "$(sha256sum /tmp/reqflux-amd64.tar.gz | cut -d' ' -f1)"
  #
  # Then set `version` and add the artifact:
  #
  #   "x86_64-linux" = {
  #     url = "https://github.com/0xErwin1/reqflux/releases/download/vX.Y.Z/reqflux-linux-amd64.tar.gz";
  #     hash = "sha256-...";
  #   };
  artifacts = { };
}
