//! ReqFlux's brand: the cobalt accent.

use bolt_legacy::Brand;
use bolt_legacy::tokens::hex;

/// The brand ReqFlux installs into Bolt.
pub(crate) fn reqflux() -> Brand {
    Brand {
        name: "ReqFlux".into(),
        accent: hex(0x2B45B8),
        accent_hover: hex(0x3451C8),
        accent_pressed: hex(0x1F3389),
        accent_tint: hex(0x9DB1FF),
        on_accent: hex(0xFFFFFF),
    }
}

/// The same brand for the shared `bolt`, which ReqFlux installs beside `bolt_legacy`.
pub(crate) fn shared() -> bolt::brand::Brand {
    let Brand {
        name,
        accent,
        accent_hover,
        accent_pressed,
        accent_tint,
        on_accent,
    } = reqflux();
    bolt::brand::Brand {
        name,
        accent,
        accent_hover,
        accent_pressed,
        accent_tint,
        on_accent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::tests::rgb_of;

    #[test]
    fn reqflux_brand_is_cobalt() {
        let brand = reqflux();
        assert_eq!(brand.name.as_ref(), "ReqFlux");
        assert_eq!(rgb_of(brand.accent), 0x2B45B8);
        assert_eq!(rgb_of(brand.accent_hover), 0x3451C8);
        assert_eq!(rgb_of(brand.accent_pressed), 0x1F3389);
        assert_eq!(rgb_of(brand.accent_tint), 0x9DB1FF);
        assert_eq!(rgb_of(brand.on_accent), 0xFFFFFF);
    }
}
