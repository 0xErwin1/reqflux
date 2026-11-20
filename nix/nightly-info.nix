# Rolling nightly channel pointer.
#
# THIS FILE IS AUTO-UPDATED by .github/workflows/nightly.yml on the `nightly`
# git ref immediately before the nightly tag is force-moved. On `main` it is
# only a seed: the hash below is a placeholder and the file will NOT fetch
# successfully from main.
#
# Consume nightly via the pinned ref:
#
#   nix run github:0xErwin1/reqflux/nightly#reqflux-nightly
#
# Do NOT pin to main for this package. The `nightly` ref always contains
# hashes that match the artifacts published in the rolling nightly release.
#
# `url` names a per-build immutable asset (reqflux-linux-amd64-<date>-<sha>-<run id>.tar.gz)
# that the workflow keeps for the 14 most recent builds, so a pin of an
# older `nightly` commit keeps working for that long. The rolling
# reqflux-linux-amd64.tar.gz asset is for humans and is never pinned.
#
# From-source nightly (no hash needed):
#
#   nix run github:0xErwin1/reqflux/nightly#reqflux-source
{
  version = "nightly";

  artifacts = {
    "x86_64-linux" = {
      url = "https://github.com/0xErwin1/reqflux/releases/download/nightly/reqflux-linux-amd64-0000-00-00-0000000-0.tar.gz";
      hash = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
    };
  };
}
