//! Acquisition summary as an XMP packet for exported images. Values come
//! from `fittle info` (canonical fields), never from raw keywords, so vendor
//! quirks are already applied. With `private`, location and serials are left
//! out entirely.

use fittle_core::Info;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// One-line human summary, e.g. `NGC 6995 · Seestar S50 · LP · 90 × 20 s`.
pub fn summary(info: &Info) -> String {
    let f = &info.fields;
    let mut parts: Vec<String> = Vec::new();
    if let Some(o) = &f.object {
        parts.push(o.value.clone());
    }
    if let Some(s) = &info.origin.scope {
        parts.push(s.name.clone());
    } else if let Some(t) = &f.telescope {
        parts.push(t.value.clone());
    }
    // A smart scope's name already says which camera it has.
    if let Some(c) = f.camera.as_ref().filter(|c| {
        !parts
            .iter()
            .any(|p| p.to_lowercase().contains(&c.value.trim().to_lowercase()))
    }) {
        parts.push(c.value.clone());
    }
    if let Some(x) = &f.filter {
        parts.push(x.value.clone());
    }
    match (&f.stack_count, &f.exposure_s) {
        (Some(n), Some(e)) if n.value > 1 => parts.push(format!("{} × {} s", n.value, e.value)),
        (_, Some(e)) => parts.push(format!("{} s", e.value)),
        _ => {}
    }
    if let Some(d) = &f.date_obs {
        parts.push(d.value.get(..10).unwrap_or(&d.value).to_string());
    }
    parts.join(" · ")
}

/// XMP packet (UTF-8) describing the source and how the export was made.
/// `processing` lists what the export did (e.g. `display stretch: auto STF`).
pub fn packet(info: &Info, processing: &[String], private: bool) -> String {
    let f = &info.fields;
    let mut props: Vec<(String, String)> = Vec::new();
    let mut put = |k: &str, v: String| props.push((k.to_string(), v));
    if let Some(v) = &f.object {
        put("fittle:Object", v.value.clone());
    }
    if let (Some(ra), Some(dec)) = (&f.ra, &f.dec) {
        put("fittle:RA", format!("{:.6}", ra.value));
        put("fittle:Dec", format!("{:.6}", dec.value));
    }
    if let Some(s) = &info.origin.scope {
        put("fittle:Scope", s.name.clone());
    }
    if !private || f.serials.iter().all(|s| s.key != "TELESCOP") {
        if let Some(v) = &f.telescope {
            put("fittle:Telescope", v.value.clone());
        }
    }
    if let Some(v) = &f.camera {
        put("fittle:Camera", v.value.clone());
    }
    if let Some(v) = &f.focal_mm {
        put("fittle:FocalLength", format!("{}", v.value));
    }
    if let Some(v) = &f.filter {
        put("fittle:Filter", v.value.clone());
    }
    if let Some(v) = &f.exposure_s {
        put("fittle:Exposure", format!("{}", v.value));
    }
    if let Some(v) = &f.stack_count {
        put("fittle:StackCount", v.value.to_string());
    }
    if let Some(v) = &f.total_integration_s {
        put("fittle:Integration", format!("{}", v.value));
    }
    if let Some(v) = &f.gain {
        put("fittle:Gain", format!("{}", v.value));
    }
    if let Some(v) = &f.date_obs {
        put("fittle:DateObs", v.value.clone());
    }
    let software: Vec<String> = info
        .origin
        .software
        .iter()
        .map(|s| s.matched.name.clone())
        .collect();
    if !software.is_empty() {
        put("fittle:Software", software.join(", "));
    }
    if !private {
        if let (Some(lat), Some(lon)) = (&f.site_lat, &f.site_lon) {
            put("fittle:SiteLatitude", format!("{:.4}", lat.value));
            put("fittle:SiteLongitude", format!("{:.4}", lon.value));
        }
    }
    if !processing.is_empty() {
        put("fittle:Processing", processing.join("; "));
    }

    let mut x = String::from(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n\
         <x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n\
         <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n\
         <rdf:Description rdf:about=\"\"\n \
         xmlns:dc=\"http://purl.org/dc/elements/1.1/\"\n \
         xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\"\n \
         xmlns:fittle=\"https://fittle.app/ns/1.0/\"\n \
         xmp:CreatorTool=\"Fittle\">\n",
    );
    let desc = summary(info);
    if let Some(o) = &f.object {
        x += &format!(
            "<dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></dc:title>\n",
            esc(&o.value)
        );
    }
    if !desc.is_empty() {
        x += &format!(
            "<dc:description><rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></dc:description>\n",
            esc(&desc)
        );
    }
    for (k, v) in props {
        x += &format!("<{k}>{}</{k}>\n", esc(&v));
    }
    x += "</rdf:Description>\n</rdf:RDF>\n</x:xmpmeta>\n<?xpacket end=\"w\"?>";
    x
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn seestar_packet_and_privacy() {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/synthetic/seestar/Light_NGC 6995_20.0s_LP_20260924-213412.fit");
        let info = fittle_core::info(&p).unwrap();
        let open = packet(&info, &["display stretch: auto STF".into()], false);
        assert!(open.contains("<fittle:Object>NGC 6995</fittle:Object>"));
        assert!(open.contains("fittle:Processing"));
        let private = packet(&info, &[], true);
        assert!(!private.contains("SiteLatitude"));
        assert!(!private.contains("S50_d2421385"), "serial leaked");
        assert!(summary(&info).starts_with("NGC 6995 · ZWO Seestar S50 · LP · 20 s"));
    }
}
