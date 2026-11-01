# Bundled fonts

Copied unchanged from DBFlux, `crates/dbflux_components/assets/fonts/`, at commit
`ff3e5153913076ec239f64fb6f99f7872f92387c` (files last changed in `b9d23f1d`).

| Family | Files | Upstream | License |
|--------|-------|----------|---------|
| Archivo | Regular, Medium, SemiBold, Bold, ExtraBold | <https://github.com/Omnibus-Type/Archivo> | SIL OFL 1.1, `Archivo-OFL.txt` |
| Archivo Expanded | Bold, ExtraBold, Black | <https://github.com/Omnibus-Type/Archivo> | SIL OFL 1.1, `Archivo-OFL.txt` |
| JetBrains Mono | Regular, Medium, SemiBold, Bold | <https://github.com/JetBrains/JetBrainsMono> | SIL OFL 1.1, `JetBrainsMono-OFL.txt` |

Only the weights a Bolt text role or control uses are bundled; no italics. `src/fonts.rs`
embeds every file listed here and a unit test checks the family name inside each one.
