//! Rig profiles: named sets of header values for a scope/camera/filter
//! combination (FOCALLEN, APTDIA, XPIXSZ, …), applied as ordinary `Set`
//! edits. Built-ins come from the smart-scope registry; user rigs live in
//! `rigs.json` in the Fittle config folder.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::edit::{NewValue, Op};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rig {
    pub name: String,
    /// Keyword → value (number, string or boolean).
    pub values: BTreeMap<String, Json>,
    /// From the scope registry (read-only).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub builtin: bool,
}

impl Rig {
    /// The `Set` edits this rig makes.
    pub fn ops(&self) -> Result<Vec<Op>, String> {
        self.values
            .iter()
            .map(|(k, v)| {
                let value = match v {
                    Json::Bool(b) => NewValue::Logical(*b),
                    Json::Number(n) if n.is_i64() => NewValue::Integer(n.as_i64().unwrap_or(0)),
                    Json::Number(n) => NewValue::Float(n.as_f64().unwrap_or(0.0)),
                    Json::String(s) => NewValue::String(s.clone()),
                    other => return Err(format!("{k}: unsupported value {other}")),
                };
                Ok(Op::Set {
                    key: k.to_uppercase(),
                    value,
                    comment: Some(format!("rig: {}", self.name)),
                })
            })
            .collect()
    }
}

/// Rigs for every registry scope with known optics.
pub fn builtin() -> Vec<Rig> {
    crate::vendor::profiles()
        .iter()
        .filter(|p| p.focal_length_mm.is_some())
        .map(|p| {
            let mut v = BTreeMap::new();
            let num = |x: f64| serde_json::Number::from_f64(x).map_or(Json::Null, Json::Number);
            if let Some(f) = p.focal_length_mm {
                v.insert("FOCALLEN".into(), num(f));
            }
            if let Some(a) = p.aperture_mm {
                v.insert("APTDIA".into(), num(a));
                if let Some(f) = p.focal_length_mm {
                    v.insert("FOCRATIO".into(), num((f / a * 100.0).round() / 100.0));
                }
            }
            if let Some(px) = p.pixel_size_um {
                v.insert("XPIXSZ".into(), num(px));
                v.insert("YPIXSZ".into(), num(px));
            }
            Rig {
                name: p.display.clone(),
                values: v,
                builtin: true,
            }
        })
        .collect()
}

/// Fittle's config folder: `FITTLE_CONFIG_DIR`, else the platform's.
pub fn config_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("FITTLE_CONFIG_DIR") {
        return Some(PathBuf::from(d));
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    if cfg!(target_os = "macos") {
        home.map(|h| h.join("Library/Application Support/Fittle"))
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("Fittle"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| home.map(|h| h.join(".config")))
            .map(|c| c.join("fittle"))
    }
}

fn file() -> Option<PathBuf> {
    config_dir().map(|d| d.join("rigs.json"))
}

/// The user's saved rigs (empty if none yet).
pub fn user() -> Vec<Rig> {
    file()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .and_then(|t| serde_json::from_str::<Vec<Rig>>(&t).ok())
        .unwrap_or_default()
}

/// Built-ins, then the user's rigs.
pub fn all() -> Vec<Rig> {
    let mut v = builtin();
    v.extend(user());
    v
}

pub fn find(name: &str) -> Option<Rig> {
    all()
        .into_iter()
        .find(|r| r.name.eq_ignore_ascii_case(name.trim()))
}

fn write(rigs: &[Rig]) -> Result<(), String> {
    let f = file().ok_or("no config folder")?;
    if let Some(d) = f.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(rigs).map_err(|e| e.to_string())?;
    // Write beside, then rename, so a crash never truncates the file.
    let tmp = f.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &f).map_err(|e| e.to_string())
}

/// Add or replace a user rig (built-in names are refused).
pub fn save(rig: Rig) -> Result<(), String> {
    if rig.name.trim().is_empty() {
        return Err("a rig needs a name".into());
    }
    if builtin()
        .iter()
        .any(|b| b.name.eq_ignore_ascii_case(&rig.name))
    {
        return Err(format!(
            "'{}' is a built-in rig; pick another name",
            rig.name
        ));
    }
    rig.ops()?;
    let mut rigs: Vec<Rig> = user()
        .into_iter()
        .filter(|r| !r.name.eq_ignore_ascii_case(&rig.name))
        .collect();
    rigs.push(Rig {
        builtin: false,
        ..rig
    });
    write(&rigs)
}

pub fn delete(name: &str) -> Result<bool, String> {
    let before = user();
    let after: Vec<Rig> = before
        .iter()
        .filter(|r| !r.name.eq_ignore_ascii_case(name))
        .cloned()
        .collect();
    if after.len() == before.len() {
        return Ok(false);
    }
    write(&after).map(|_| true)
}

/// A rig capturing `keys` from a header (values as written).
pub fn from_header(name: &str, h: &crate::Header, keys: &[&str]) -> Rig {
    let mut values = BTreeMap::new();
    for k in keys {
        let Some(c) = h.get(&k.to_uppercase()) else {
            continue;
        };
        let v = match &c.value {
            crate::Value::Logical(b) => Json::Bool(*b),
            crate::Value::Integer(i) => Json::from(*i),
            crate::Value::Float(f) => {
                serde_json::Number::from_f64(*f).map_or(Json::Null, Json::Number)
            }
            crate::Value::String(s) => Json::String(s.trim().to_string()),
            _ => continue,
        };
        values.insert(c.keyword.clone(), v);
    }
    Rig {
        name: name.to_string(),
        values,
        builtin: false,
    }
}

/// Keywords a rig captures by default.
pub const RIG_KEYS: &[&str] = &[
    "TELESCOP", "INSTRUME", "FOCALLEN", "APTDIA", "FOCRATIO", "XPIXSZ", "YPIXSZ", "FILTER", "GAIN",
    "OFFSET", "XBINNING", "YBINNING",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_from_registry() {
        let s50 = builtin()
            .into_iter()
            .find(|r| r.name == "ZWO Seestar S50")
            .unwrap();
        assert_eq!(s50.values["FOCALLEN"], 250.0);
        assert_eq!(s50.values["APTDIA"], 50.0);
        assert_eq!(s50.values["XPIXSZ"], 2.9);
        assert_eq!(s50.values["FOCRATIO"], 5.0);
        let ops = s50.ops().unwrap();
        assert!(ops.iter().any(|o| matches!(o, Op::Set { key, value: NewValue::Float(v), .. } if key == "FOCALLEN" && *v == 250.0)));
    }

    #[test]
    fn save_find_delete() {
        let dir = std::env::temp_dir().join(format!("fittle-rigs-{}", std::process::id()));
        // SAFETY: tests in this module don't read the variable concurrently.
        unsafe { std::env::set_var("FITTLE_CONFIG_DIR", &dir) };
        let mut values = BTreeMap::new();
        values.insert("FOCALLEN".into(), Json::from(530.0));
        values.insert("TELESCOP".into(), Json::from("RedCat 71"));
        save(Rig {
            name: "RedCat · L-eXtreme".into(),
            values,
            builtin: false,
        })
        .unwrap();
        assert_eq!(
            find("redcat · l-extreme").unwrap().values["TELESCOP"],
            "RedCat 71"
        );
        assert!(
            save(Rig {
                name: "ZWO Seestar S50".into(),
                values: BTreeMap::new(),
                builtin: false
            })
            .is_err()
        );
        assert!(delete("RedCat · L-eXtreme").unwrap());
        assert!(find("RedCat · L-eXtreme").is_none());
        let _ = std::fs::remove_dir_all(dir);
    }
}
