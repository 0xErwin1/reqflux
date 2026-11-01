//! The bundled fonts: Archivo, Archivo Expanded and JetBrains Mono, under the OFL.
//!
//! See `assets/fonts/README.md` for their provenance.

use std::borrow::Cow;

use gpui::App;

use crate::tokens::family;

/// One embedded font file.
#[derive(Debug, Clone, Copy)]
pub struct FontAsset {
    /// The family the file registers.
    pub family: &'static str,
    /// The file name under `assets/fonts/`.
    pub file_name: &'static str,
    /// The file contents.
    pub data: &'static [u8],
}

macro_rules! font {
    ($family:expr, $file:literal) => {
        FontAsset {
            family: $family,
            file_name: $file,
            data: include_bytes!(concat!("../assets/fonts/", $file)),
        }
    };
}

/// Every bundled font file. Only the weights a text role or control uses are included.
pub const FONT_ASSETS: [FontAsset; 12] = [
    font!(family::INTERFACE, "Archivo-Regular.ttf"),
    font!(family::INTERFACE, "Archivo-Medium.ttf"),
    font!(family::INTERFACE, "Archivo-SemiBold.ttf"),
    font!(family::INTERFACE, "Archivo-Bold.ttf"),
    font!(family::INTERFACE, "Archivo-ExtraBold.ttf"),
    font!(family::DISPLAY, "ArchivoExpanded-Bold.ttf"),
    font!(family::DISPLAY, "ArchivoExpanded-ExtraBold.ttf"),
    font!(family::DISPLAY, "ArchivoExpanded-Black.ttf"),
    font!(family::MONO, "JetBrainsMono-Regular.ttf"),
    font!(family::MONO, "JetBrainsMono-Medium.ttf"),
    font!(family::MONO, "JetBrainsMono-SemiBold.ttf"),
    font!(family::MONO, "JetBrainsMono-Bold.ttf"),
];

/// The families of [`FONT_ASSETS`].
pub const FAMILIES: [&str; 3] = [family::INTERFACE, family::DISPLAY, family::MONO];

/// Registers the bundled fonts with GPUI's text system.
///
/// A failure is logged and leaves GPUI on its fallback fonts, so the app still starts.
pub(crate) fn load(cx: &mut App) {
    let data = FONT_ASSETS
        .iter()
        .map(|asset| Cow::Borrowed(asset.data))
        .collect();
    if let Err(error) = cx.text_system().add_fonts(data) {
        log::error!("Bundled fonts failed to register, using fallback fonts: {error}");
        return;
    }

    let available = cx.text_system().all_font_names();
    let missing = missing_families(&available);
    if !missing.is_empty() {
        log::warn!("Bundled font families not available after registration: {missing:?}");
    }
}

/// The bundled families absent from `available`.
pub fn missing_families(available: &[String]) -> Vec<&'static str> {
    FAMILIES
        .into_iter()
        .filter(|family| !available.iter().any(|name| name == family))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The typographic family (name id 16, else 1) in the `name` table of a TrueType file,
    /// read from its Windows Unicode records.
    fn family_name(data: &[u8]) -> Option<String> {
        let u16_at = |at: usize| Some(u16::from_be_bytes([*data.get(at)?, *data.get(at + 1)?]));
        let u32_at = |at: usize| Some(((u16_at(at)? as usize) << 16) | u16_at(at + 2)? as usize);

        let tables = u16_at(4)? as usize;
        let name = (0..tables)
            .map(|index| 12 + index * 16)
            .find(|record| data.get(*record..record + 4) == Some(b"name"))
            .and_then(|record| u32_at(record + 8))?;

        let count = u16_at(name + 2)? as usize;
        let strings = name + u16_at(name + 4)? as usize;
        let read = |wanted: u16| {
            (0..count).find_map(|index| {
                let record = name + 6 + index * 12;
                if u16_at(record)? != 3 || u16_at(record + 6)? != wanted {
                    return None;
                }
                let length = u16_at(record + 8)? as usize;
                let start = strings + u16_at(record + 10)? as usize;
                let units: Vec<u16> = data
                    .get(start..start + length)?
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| u16::from_be_bytes(*pair))
                    .collect();
                String::from_utf16(&units).ok()
            })
        };
        read(16).or_else(|| read(1))
    }

    #[test]
    fn every_asset_registers_its_declared_family() {
        for asset in FONT_ASSETS {
            assert_eq!(
                family_name(asset.data).as_deref(),
                Some(asset.family),
                "{}",
                asset.file_name
            );
        }
    }

    #[test]
    fn every_family_has_a_bundled_file() {
        for family in FAMILIES {
            assert!(
                FONT_ASSETS.iter().any(|asset| asset.family == family),
                "{family}"
            );
        }
    }

    #[test]
    fn missing_families_names_the_absent_ones() {
        let available = vec!["Archivo".to_string(), "JetBrains Mono".to_string()];
        assert_eq!(missing_families(&available), vec!["Archivo Expanded"]);
        let all: Vec<String> = FAMILIES.iter().map(|f| f.to_string()).collect();
        assert!(missing_families(&all).is_empty());
    }
}
