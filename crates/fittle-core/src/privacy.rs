//! Privacy scrub: the edits that remove location, identity and serial
//! numbers before a file is shared. Computed as ordinary `Op`s, so it goes
//! through the same plan, diff and safe write as any edit.

use crate::edit::{NewValue, Op};
use crate::fits::Fits;

/// Keywords that locate or identify the observer or the hardware unit.
pub const PRIVATE_KEYS: &[(&str, &str)] = &[
    ("SITELAT", "site latitude"),
    ("SITELONG", "site longitude"),
    ("SITEELEV", "site elevation"),
    ("OBSLAT", "site latitude"),
    ("OBSLONG", "site longitude"),
    ("OBSALT", "site elevation"),
    ("LAT-OBS", "site latitude"),
    ("LONG-OBS", "site longitude"),
    ("ALT-OBS", "site elevation"),
    ("OBSGEO-B", "site latitude"),
    ("OBSGEO-L", "site longitude"),
    ("OBSGEO-H", "site elevation"),
    ("OBSGEO-X", "site position"),
    ("OBSGEO-Y", "site position"),
    ("OBSGEO-Z", "site position"),
    ("LATITUDE", "site latitude (Unistellar)"),
    ("LONGITUD", "site longitude (Unistellar)"),
    ("ALTITUDE", "site elevation (Unistellar)"),
    ("OBSERVER", "observer name"),
    ("AUTHOR", "author name"),
    ("OBSERVAT", "observatory name"),
    ("SITENAME", "site name"),
    ("CAMERAID", "camera serial"),
    ("SERIALNB", "device serial"),
    ("SERIALNO", "device serial"),
    ("CAMSERNO", "camera serial"),
];

/// Edits that scrub one file's image HDU (and primary, if different).
pub fn scrub_ops(fits: &Fits, path: &str) -> Vec<Op> {
    let info = crate::info::info_from(fits, path);
    let hdu = crate::edit::default_hdu(fits);
    let h = fits.hdus[hdu].header();
    let mut ops: Vec<Op> = PRIVATE_KEYS
        .iter()
        .filter(|(k, _)| h.get(k).is_some())
        .map(|(k, _)| Op::Unset { key: k.to_string() })
        .collect();
    // Serial numbers written in place of a name (Seestar TELESCOP): replace
    // with the model name when the scope is known, else remove.
    for s in &info.fields.serials {
        if ops
            .iter()
            .any(|o| matches!(o, Op::Unset { key } if *key == s.key))
        {
            continue;
        }
        ops.push(match &info.origin.scope {
            Some(scope) => Op::Set {
                key: s.key.clone(),
                value: NewValue::String(scope.name.clone()),
                comment: Some("serial removed by privacy scrub".into()),
            },
            None => Op::Unset { key: s.key.clone() },
        });
    }
    ops
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn seestar_scrub() {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/synthetic/seestar/Light_NGC 6995_20.0s_LP_20260924-213412.fit");
        let fits = Fits::open(&p).unwrap();
        let ops = scrub_ops(&fits, &p.to_string_lossy());
        assert!(ops.contains(&Op::Unset {
            key: "SITELAT".into()
        }));
        assert!(ops.contains(&Op::Unset {
            key: "SITELONG".into()
        }));
    }
}
