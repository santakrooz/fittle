//! Deep-sky catalogue lookups over the OpenNGC subset shared with
//! AstroSideKick (`data/targets*.json`, CC BY-SA 4.0; see data/SOURCES.md).

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
struct Row {
    n: Vec<String>,
    t: String,
    ra: f64,
    dec: f64,
    size: Option<f64>,
    c: Option<String>,
}

/// A catalogue object as reported to users.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Target {
    /// Primary designation, e.g. `NGC 6995`.
    pub id: String,
    /// Common name if known, e.g. `Eastern Veil`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub common_name: Option<String>,
    /// All designations in the catalogue.
    pub aliases: Vec<String>,
    /// Human label for the OpenNGC type, e.g. `Supernova remnant`.
    pub kind: String,
    pub ra: f64,
    pub dec: f64,
    /// Major axis in arcminutes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_arcmin: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constellation: Option<String>,
}

struct Catalog {
    rows: Vec<Row>,
    by_name: HashMap<String, usize>,
    names: HashMap<String, String>,
}

fn catalog() -> &'static Catalog {
    static CAT: OnceLock<Catalog> = OnceLock::new();
    CAT.get_or_init(|| {
        let mut rows: Vec<Row> =
            serde_json::from_str(include_str!("../data/targets.json")).expect("targets.json");
        let extra: Vec<Row> = serde_json::from_str(include_str!("../data/targets-extra.json"))
            .expect("targets-extra.json");
        rows.extend(extra);
        let names: HashMap<String, String> =
            serde_json::from_str(include_str!("../data/targets-names.json"))
                .expect("targets-names.json");
        let mut by_name = HashMap::new();
        for (i, r) in rows.iter().enumerate() {
            for n in &r.n {
                by_name.entry(normalize(n)).or_insert(i);
            }
        }
        for (id, common) in &names {
            if let Some(&i) = by_name.get(&normalize(id)) {
                by_name.entry(normalize(common)).or_insert(i);
            }
        }
        Catalog {
            rows,
            by_name,
            names,
        }
    })
}

/// `NGC 7380`, `ngc7380`, `NGC-7380` → `NGC7380`; `M 031` → `M31`.
fn normalize(name: &str) -> String {
    let compact: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '+')
        .collect::<String>()
        .to_uppercase();
    // Strip leading zeros from the numeric part (M031 → M31).
    let split = compact
        .find(|c: char| c.is_ascii_digit())
        .unwrap_or(compact.len());
    let (prefix, num) = compact.split_at(split);
    let trimmed = num.trim_start_matches('0');
    if num.chars().all(|c| c.is_ascii_digit()) && !trimmed.is_empty() {
        format!("{prefix}{trimmed}")
    } else {
        compact
    }
}

fn to_target(r: &Row) -> Target {
    let cat = catalog();
    let common =
        r.n.iter()
            .find_map(|n| cat.names.get(n).cloned())
            .or_else(|| {
                // Rows often carry the common name as a non-catalogue alias.
                r.n.iter()
                    .skip(1)
                    .find(|n| !n.chars().any(|c| c.is_ascii_digit()))
                    .cloned()
            });
    Target {
        id: r.n[0].clone(),
        common_name: common,
        aliases: r.n.clone(),
        kind: kind_label(&r.t).to_string(),
        ra: r.ra,
        dec: r.dec,
        size_arcmin: r.size,
        constellation: r
            .c
            .as_deref()
            .map(|c| constellation_name(c).unwrap_or(c).to_string()),
    }
}

/// Look up an OBJECT value by designation or common name.
pub fn lookup(name: &str) -> Option<Target> {
    let cat = catalog();
    cat.by_name
        .get(&normalize(name))
        .map(|&i| to_target(&cat.rows[i]))
}

/// Nearest catalogue object to a position within `radius_deg`, preferring
/// larger objects when several are close (the one the frame is "of").
pub fn nearest(ra: f64, dec: f64, radius_deg: f64) -> Option<(Target, f64)> {
    let here = fittle_astro::Equatorial { ra, dec };
    catalog()
        .rows
        .iter()
        .filter_map(|r| {
            let d = fittle_astro::angular_separation(
                here,
                fittle_astro::Equatorial {
                    ra: r.ra,
                    dec: r.dec,
                },
            );
            (d <= radius_deg).then_some((r, d))
        })
        .min_by(|(a, da), (b, db)| {
            let score = |r: &Row, d: f64| d - r.size.unwrap_or(0.0) / 120.0;
            score(a, *da).total_cmp(&score(b, *db))
        })
        .map(|(r, d)| (to_target(r), d))
}

fn kind_label(t: &str) -> &str {
    match t {
        "G" => "Galaxy",
        "GPair" => "Galaxy pair",
        "GTrpl" => "Galaxy triplet",
        "GGroup" => "Galaxy group",
        "OCl" => "Open cluster",
        "GCl" => "Globular cluster",
        "Cl+N" => "Cluster with nebula",
        "*Ass" => "Stellar association",
        "PN" => "Planetary nebula",
        "HII" => "Emission nebula (HII region)",
        "EmN" => "Emission nebula",
        "RfN" => "Reflection nebula",
        "DrkN" => "Dark nebula",
        "Neb" => "Nebula",
        "SNR" => "Supernova remnant",
        "Nova" => "Nova",
        "*" => "Star",
        "**" => "Double star",
        _ => "Other",
    }
}

/// IAU abbreviation → name.
pub fn constellation_name(abbr: &str) -> Option<&'static str> {
    const TABLE: [(&str, &str); 88] = [
        ("And", "Andromeda"),
        ("Ant", "Antlia"),
        ("Aps", "Apus"),
        ("Aqr", "Aquarius"),
        ("Aql", "Aquila"),
        ("Ara", "Ara"),
        ("Ari", "Aries"),
        ("Aur", "Auriga"),
        ("Boo", "Boötes"),
        ("Cae", "Caelum"),
        ("Cam", "Camelopardalis"),
        ("Cnc", "Cancer"),
        ("CVn", "Canes Venatici"),
        ("CMa", "Canis Major"),
        ("CMi", "Canis Minor"),
        ("Cap", "Capricornus"),
        ("Car", "Carina"),
        ("Cas", "Cassiopeia"),
        ("Cen", "Centaurus"),
        ("Cep", "Cepheus"),
        ("Cet", "Cetus"),
        ("Cha", "Chamaeleon"),
        ("Cir", "Circinus"),
        ("Col", "Columba"),
        ("Com", "Coma Berenices"),
        ("CrA", "Corona Australis"),
        ("CrB", "Corona Borealis"),
        ("Crv", "Corvus"),
        ("Crt", "Crater"),
        ("Cru", "Crux"),
        ("Cyg", "Cygnus"),
        ("Del", "Delphinus"),
        ("Dor", "Dorado"),
        ("Dra", "Draco"),
        ("Equ", "Equuleus"),
        ("Eri", "Eridanus"),
        ("For", "Fornax"),
        ("Gem", "Gemini"),
        ("Gru", "Grus"),
        ("Her", "Hercules"),
        ("Hor", "Horologium"),
        ("Hya", "Hydra"),
        ("Hyi", "Hydrus"),
        ("Ind", "Indus"),
        ("Lac", "Lacerta"),
        ("Leo", "Leo"),
        ("LMi", "Leo Minor"),
        ("Lep", "Lepus"),
        ("Lib", "Libra"),
        ("Lup", "Lupus"),
        ("Lyn", "Lynx"),
        ("Lyr", "Lyra"),
        ("Men", "Mensa"),
        ("Mic", "Microscopium"),
        ("Mon", "Monoceros"),
        ("Mus", "Musca"),
        ("Nor", "Norma"),
        ("Oct", "Octans"),
        ("Oph", "Ophiuchus"),
        ("Ori", "Orion"),
        ("Pav", "Pavo"),
        ("Peg", "Pegasus"),
        ("Per", "Perseus"),
        ("Phe", "Phoenix"),
        ("Pic", "Pictor"),
        ("Psc", "Pisces"),
        ("PsA", "Piscis Austrinus"),
        ("Pup", "Puppis"),
        ("Pyx", "Pyxis"),
        ("Ret", "Reticulum"),
        ("Sge", "Sagitta"),
        ("Sgr", "Sagittarius"),
        ("Sco", "Scorpius"),
        ("Scl", "Sculptor"),
        ("Sct", "Scutum"),
        ("Ser", "Serpens"),
        ("Sex", "Sextans"),
        ("Tau", "Taurus"),
        ("Tel", "Telescopium"),
        ("Tri", "Triangulum"),
        ("TrA", "Triangulum Australe"),
        ("Tuc", "Tucana"),
        ("UMa", "Ursa Major"),
        ("UMi", "Ursa Minor"),
        ("Vel", "Vela"),
        ("Vir", "Virgo"),
        ("Vol", "Volans"),
        ("Vul", "Vulpecula"),
    ];
    // OpenNGC splits Serpens into Caput (Se1) and Cauda (Se2).
    let abbr = if abbr.eq_ignore_ascii_case("Se1") || abbr.eq_ignore_ascii_case("Se2") {
        "Ser"
    } else {
        abbr
    };
    TABLE
        .iter()
        .find(|(a, _)| a.eq_ignore_ascii_case(abbr))
        .map(|(_, n)| *n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_normalize() {
        assert_eq!(normalize("NGC 7380"), "NGC7380");
        assert_eq!(normalize("m 031"), "M31");
        assert_eq!(normalize("IC-1396"), "IC1396");
    }

    #[test]
    fn lookups() {
        let t = lookup("NGC 7380").unwrap();
        assert_eq!(t.constellation.as_deref(), Some("Cepheus"));
        let m31 = lookup("M31").unwrap();
        assert_eq!(m31.kind, "Galaxy");
        assert_eq!(m31.constellation.as_deref(), Some("Andromeda"));
        assert!(lookup("Horsehead Nebula").is_some());
        assert!(lookup("not a thing").is_none());
    }

    #[test]
    fn nearest_to_pointing() {
        // Seestar RA/Dec written for NGC 7380.
        let (t, d) = nearest(342.11667, 58.280833, 1.0).unwrap();
        assert!(t.aliases.iter().any(|a| a == "NGC 7380"), "{t:?}");
        assert!(d < 0.5);
    }

    #[test]
    fn constellations() {
        assert_eq!(constellation_name("cyg"), Some("Cygnus"));
        assert_eq!(constellation_name("Xyz"), None);
        assert_eq!(constellation_name("Se2"), Some("Serpens"));
    }
}
