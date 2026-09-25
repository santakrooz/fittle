//! Keyword dictionary: plain-English label, unit, group and "why it matters"
//! help for FITS standard keys and common astrophotography conventions.
//!
//! This is the M0 stub. Vendor quirks (Seestar, ASIAIR, N.I.N.A. …) land in M1.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    Structure,
    Target,
    Optics,
    Camera,
    Filter,
    Exposure,
    Time,
    Site,
    Mount,
    Astrometry,
    Processing,
    Software,
    Other,
}

impl Group {
    pub const ALL: [Group; 13] = [
        Group::Structure,
        Group::Target,
        Group::Optics,
        Group::Camera,
        Group::Filter,
        Group::Exposure,
        Group::Time,
        Group::Site,
        Group::Mount,
        Group::Astrometry,
        Group::Processing,
        Group::Software,
        Group::Other,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Group::Structure => "Structure",
            Group::Target => "Target",
            Group::Optics => "Optics",
            Group::Camera => "Camera",
            Group::Filter => "Filter",
            Group::Exposure => "Exposure",
            Group::Time => "Time",
            Group::Site => "Site",
            Group::Mount => "Mount / guiding",
            Group::Astrometry => "Astrometry",
            Group::Processing => "Processing",
            Group::Software => "Software",
            Group::Other => "Other",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct KeywordInfo {
    /// Keyword, with `n` / `i_j` standing for axis indices (e.g. `NAXISn`).
    pub keyword: &'static str,
    pub label: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<&'static str>,
    pub group: Group,
    pub help: &'static str,
    /// Structural keys describe the data layout and are never editable.
    pub structural: bool,
}

const fn k(
    keyword: &'static str,
    label: &'static str,
    unit: Option<&'static str>,
    group: Group,
    help: &'static str,
) -> KeywordInfo {
    KeywordInfo {
        keyword,
        label,
        unit,
        group,
        help,
        structural: false,
    }
}

const fn s(keyword: &'static str, label: &'static str, help: &'static str) -> KeywordInfo {
    KeywordInfo {
        keyword,
        label,
        unit: None,
        group: Group::Structure,
        help,
        structural: true,
    }
}

use Group::*;

pub static KEYWORDS: &[KeywordInfo] = &[
    // Structure (locked)
    s(
        "SIMPLE",
        "Conforms to FITS",
        "T when the file follows the FITS standard.",
    ),
    s(
        "XTENSION",
        "Extension type",
        "Type of this extension HDU: IMAGE, TABLE or BINTABLE.",
    ),
    s(
        "BITPIX",
        "Bits per pixel",
        "Pixel type: 8/16/32/64 are integers, -32/-64 are floats. Stacks are usually -32.",
    ),
    s(
        "NAXIS",
        "Number of axes",
        "2 for a mono or CFA image, 3 for an RGB cube.",
    ),
    s(
        "NAXISn",
        "Axis length",
        "Length of axis n in pixels (NAXIS1 is width, NAXIS2 height).",
    ),
    s(
        "PCOUNT",
        "Parameter count",
        "Size of the heap after a binary table.",
    ),
    s(
        "GCOUNT",
        "Group count",
        "Number of groups; 1 for images and tables.",
    ),
    s(
        "EXTEND",
        "May have extensions",
        "T when extension HDUs may follow the primary.",
    ),
    s(
        "GROUPS",
        "Random groups",
        "Legacy random-groups format flag.",
    ),
    s("TFIELDS", "Table columns", "Number of columns in a table."),
    s("TFORMn", "Column format", "Data type of table column n."),
    s(
        "TBCOLn",
        "Column start",
        "Start byte of ASCII table column n.",
    ),
    s(
        "THEAP",
        "Heap offset",
        "Byte offset of the binary-table heap.",
    ),
    s(
        "ZIMAGE",
        "Compressed image",
        "T when this table holds a tile-compressed image.",
    ),
    s(
        "ZBITPIX",
        "Original bits per pixel",
        "BITPIX of the image before compression.",
    ),
    s(
        "ZNAXIS",
        "Original number of axes",
        "NAXIS of the image before compression.",
    ),
    s(
        "ZNAXISn",
        "Original axis length",
        "Length of axis n before compression.",
    ),
    s("ZTILEn", "Tile size", "Compression tile size along axis n."),
    s(
        "ZCMPTYPE",
        "Compression",
        "Tile compression algorithm, e.g. RICE_1.",
    ),
    k(
        "BZERO",
        "Zero offset",
        None,
        Structure,
        "Physical = BZERO + BSCALE × stored. 32768 means unsigned 16-bit.",
    ),
    k(
        "BSCALE",
        "Scale factor",
        None,
        Structure,
        "Physical = BZERO + BSCALE × stored.",
    ),
    k(
        "BUNIT",
        "Pixel unit",
        None,
        Structure,
        "Physical unit of pixel values, e.g. ADU.",
    ),
    k(
        "DATAMIN",
        "Data minimum",
        None,
        Structure,
        "Minimum pixel value, if written.",
    ),
    k(
        "DATAMAX",
        "Data maximum",
        None,
        Structure,
        "Maximum pixel value, if written.",
    ),
    k(
        "CHECKSUM",
        "HDU checksum",
        None,
        Structure,
        "Checksum of the whole HDU; must be recomputed after edits.",
    ),
    k(
        "DATASUM",
        "Data checksum",
        None,
        Structure,
        "Checksum of the data unit only.",
    ),
    k(
        "EXTNAME",
        "Extension name",
        None,
        Structure,
        "Name of this HDU.",
    ),
    k(
        "ROWORDER",
        "Row order",
        None,
        Structure,
        "TOP-DOWN or BOTTOM-UP; affects how the image and Bayer pattern are oriented.",
    ),
    k(
        "TTYPEn",
        "Column name",
        None,
        Structure,
        "Name of table column n.",
    ),
    k(
        "TUNITn",
        "Column unit",
        None,
        Structure,
        "Unit of table column n.",
    ),
    // Target
    k(
        "OBJECT",
        "Object",
        None,
        Target,
        "Target name as entered in the capture app.",
    ),
    k(
        "OBJCTRA",
        "Object RA",
        Some("h m s"),
        Target,
        "Right ascension of the target, sexagesimal.",
    ),
    k(
        "OBJCTDEC",
        "Object Dec",
        Some("d m s"),
        Target,
        "Declination of the target, sexagesimal.",
    ),
    k(
        "RA",
        "RA",
        Some("deg"),
        Target,
        "Right ascension of the pointing, degrees.",
    ),
    k(
        "DEC",
        "Dec",
        Some("deg"),
        Target,
        "Declination of the pointing, degrees.",
    ),
    k(
        "EQUINOX",
        "Equinox",
        Some("yr"),
        Target,
        "Equinox of the coordinates, usually 2000.",
    ),
    k(
        "OBJCTROT",
        "Rotation",
        Some("deg"),
        Target,
        "Camera or field rotation angle.",
    ),
    // Optics
    k(
        "TELESCOP",
        "Telescope",
        None,
        Optics,
        "Telescope or smart-scope model.",
    ),
    k(
        "FOCALLEN",
        "Focal length",
        Some("mm"),
        Optics,
        "Effective focal length. Drives pixel scale; a wrong value is a common error.",
    ),
    k(
        "APTDIA",
        "Aperture",
        Some("mm"),
        Optics,
        "Aperture diameter.",
    ),
    k(
        "APERTURE",
        "Aperture",
        Some("mm"),
        Optics,
        "Aperture diameter (alternative key).",
    ),
    k(
        "APTAREA",
        "Aperture area",
        Some("mm²"),
        Optics,
        "Clear aperture area.",
    ),
    k(
        "FOCRATIO",
        "Focal ratio",
        None,
        Optics,
        "Focal length divided by aperture.",
    ),
    // Camera
    k(
        "INSTRUME",
        "Camera",
        None,
        Camera,
        "Camera or sensor model.",
    ),
    k(
        "XPIXSZ",
        "Pixel width",
        Some("µm"),
        Camera,
        "Pixel size including binning. Drives pixel scale.",
    ),
    k(
        "YPIXSZ",
        "Pixel height",
        Some("µm"),
        Camera,
        "Pixel size including binning.",
    ),
    k(
        "PIXSIZE1",
        "Pixel width",
        Some("µm"),
        Camera,
        "Pixel size (alternative key).",
    ),
    k(
        "PIXSIZE2",
        "Pixel height",
        Some("µm"),
        Camera,
        "Pixel size (alternative key).",
    ),
    k(
        "XBINNING",
        "Binning X",
        None,
        Camera,
        "Horizontal binning factor.",
    ),
    k(
        "YBINNING",
        "Binning Y",
        None,
        Camera,
        "Vertical binning factor.",
    ),
    k(
        "GAIN",
        "Gain",
        None,
        Camera,
        "Camera gain setting. Calibration frames must match.",
    ),
    k(
        "OFFSET",
        "Offset",
        None,
        Camera,
        "Camera offset (black level). Calibration frames must match.",
    ),
    k(
        "EGAIN",
        "Electrons per ADU",
        Some("e-/ADU"),
        Camera,
        "Conversion gain at this gain setting.",
    ),
    k(
        "CCD-TEMP",
        "Sensor temperature",
        Some("°C"),
        Camera,
        "Measured sensor temperature. Darks should match within ~2 °C.",
    ),
    k(
        "SET-TEMP",
        "Cooler set point",
        Some("°C"),
        Camera,
        "Target temperature of the cooler.",
    ),
    k(
        "BAYERPAT",
        "Bayer pattern",
        None,
        Camera,
        "CFA layout (RGGB, GRBG, …) for one-shot-colour sensors.",
    ),
    k(
        "XBAYROFF",
        "Bayer X offset",
        None,
        Camera,
        "Column offset of the Bayer pattern.",
    ),
    k(
        "YBAYROFF",
        "Bayer Y offset",
        None,
        Camera,
        "Row offset of the Bayer pattern.",
    ),
    k(
        "READOUTM",
        "Readout mode",
        None,
        Camera,
        "Camera readout mode.",
    ),
    k(
        "CAMERAID",
        "Camera ID",
        None,
        Camera,
        "Camera serial or identifier.",
    ),
    // Filter
    k(
        "FILTER",
        "Filter",
        None,
        Filter,
        "Filter name. Flats must match.",
    ),
    k("FWHEEL", "Filter wheel", None, Filter, "Filter wheel name."),
    // Exposure
    k(
        "IMAGETYP",
        "Frame type",
        None,
        Exposure,
        "Light, Dark, Flat, Bias … as written by the capture app.",
    ),
    k(
        "FRAME",
        "Frame type",
        None,
        Exposure,
        "Frame type (alternative key).",
    ),
    k(
        "EXPTIME",
        "Exposure",
        Some("s"),
        Exposure,
        "Exposure time of this frame.",
    ),
    k(
        "EXPOSURE",
        "Exposure",
        Some("s"),
        Exposure,
        "Exposure time (alternative key).",
    ),
    k(
        "STACKCNT",
        "Stacked frames",
        None,
        Exposure,
        "Number of subs combined into this image.",
    ),
    k(
        "NCOMBINE",
        "Combined frames",
        None,
        Exposure,
        "Number of frames combined.",
    ),
    k(
        "LIVETIME",
        "Total integration",
        Some("s"),
        Exposure,
        "Sum of exposure times of combined frames.",
    ),
    // Time
    k(
        "DATE-OBS",
        "Start (UTC)",
        None,
        Time,
        "Exposure start time in UTC.",
    ),
    k(
        "DATE-LOC",
        "Start (local)",
        None,
        Time,
        "Exposure start in local time.",
    ),
    k(
        "DATE-END",
        "End (UTC)",
        None,
        Time,
        "Exposure end time in UTC.",
    ),
    k(
        "DATE",
        "File written",
        None,
        Time,
        "When this HDU was written.",
    ),
    k(
        "MJD-OBS",
        "Start (MJD)",
        Some("d"),
        Time,
        "Modified Julian Date of exposure start.",
    ),
    k(
        "JD",
        "Start (JD)",
        Some("d"),
        Time,
        "Julian Date of exposure start.",
    ),
    // Site
    k(
        "SITELAT",
        "Site latitude",
        Some("deg"),
        Site,
        "Observer latitude. Private: scrub before sharing.",
    ),
    k(
        "SITELONG",
        "Site longitude",
        Some("deg"),
        Site,
        "Observer longitude, east positive. Private: scrub before sharing.",
    ),
    k(
        "SITEELEV",
        "Site elevation",
        Some("m"),
        Site,
        "Observer elevation above sea level.",
    ),
    k(
        "OBSERVAT",
        "Observatory",
        None,
        Site,
        "Observatory or site name.",
    ),
    k(
        "OBSERVER",
        "Observer",
        None,
        Site,
        "Observer name. Private: scrub before sharing.",
    ),
    // Mount / guiding
    k("MOUNT", "Mount", None, Mount, "Mount model."),
    k(
        "PIERSIDE",
        "Pier side",
        None,
        Mount,
        "EAST or WEST. Flats taken on the other side may not match after a flip.",
    ),
    k(
        "FOCPOS",
        "Focuser position",
        Some("steps"),
        Mount,
        "Focuser position.",
    ),
    k(
        "FOCUSPOS",
        "Focuser position",
        Some("steps"),
        Mount,
        "Focuser position (alternative key).",
    ),
    k(
        "FOCTEMP",
        "Focuser temperature",
        Some("°C"),
        Mount,
        "Temperature at the focuser; focus drifts with it.",
    ),
    k("FOCNAME", "Focuser", None, Mount, "Focuser model."),
    k(
        "AIRMASS",
        "Airmass",
        None,
        Mount,
        "Airmass at exposure start.",
    ),
    k(
        "CENTALT",
        "Altitude",
        Some("deg"),
        Mount,
        "Target altitude at exposure start.",
    ),
    k(
        "CENTAZ",
        "Azimuth",
        Some("deg"),
        Mount,
        "Target azimuth at exposure start.",
    ),
    // Astrometry
    k(
        "CTYPEn",
        "Axis type",
        None,
        Astrometry,
        "WCS projection for axis n, e.g. RA---TAN.",
    ),
    k(
        "CRVALn",
        "Reference value",
        Some("deg"),
        Astrometry,
        "Sky coordinate at the reference pixel.",
    ),
    k(
        "CRPIXn",
        "Reference pixel",
        Some("px"),
        Astrometry,
        "Pixel coordinate of the reference point.",
    ),
    k(
        "CDELTn",
        "Pixel increment",
        Some("deg/px"),
        Astrometry,
        "Degrees per pixel along axis n.",
    ),
    k(
        "CROTAn",
        "Rotation",
        Some("deg"),
        Astrometry,
        "Legacy WCS rotation.",
    ),
    k(
        "CUNITn",
        "Axis unit",
        None,
        Astrometry,
        "Unit of WCS axis n.",
    ),
    k(
        "CDi_j",
        "CD matrix",
        Some("deg/px"),
        Astrometry,
        "WCS linear transform element.",
    ),
    k(
        "PCi_j",
        "PC matrix",
        None,
        Astrometry,
        "WCS rotation/skew matrix element.",
    ),
    k(
        "RADESYS",
        "Reference frame",
        None,
        Astrometry,
        "Celestial reference frame, e.g. ICRS or FK5.",
    ),
    k(
        "PLTSOLVD",
        "Plate solved",
        None,
        Astrometry,
        "T when the image was plate solved.",
    ),
    k(
        "SCALE",
        "Pixel scale",
        Some("arcsec/px"),
        Astrometry,
        "Pixel scale as written by the capture app.",
    ),
    // Processing
    k(
        "CALSTAT",
        "Calibration",
        None,
        Processing,
        "Calibration steps applied (B, D, F).",
    ),
    k(
        "PEDESTAL",
        "Pedestal",
        Some("ADU"),
        Processing,
        "Offset added during calibration.",
    ),
    k(
        "CVF",
        "Conversion factor",
        Some("e-/ADU"),
        Processing,
        "Electrons per ADU after processing.",
    ),
    // Software
    k(
        "SWCREATE",
        "Created by",
        None,
        Software,
        "Software that wrote the file. Used to identify the capture app.",
    ),
    k(
        "SWMODIFY",
        "Modified by",
        None,
        Software,
        "Software that last modified the file.",
    ),
    k(
        "CREATOR",
        "Creator",
        None,
        Software,
        "Software that wrote the file (alternative key).",
    ),
    k(
        "PROGRAM",
        "Program",
        None,
        Software,
        "Software that wrote the file (alternative key).",
    ),
    k(
        "SOFTWARE",
        "Software",
        None,
        Software,
        "Software that wrote the file (alternative key).",
    ),
    k(
        "ORIGIN",
        "Origin",
        None,
        Software,
        "Organisation or software that created the file.",
    ),
];

/// Look up a keyword, mapping indexed keys (`NAXIS2`, `CD1_2`) to their
/// pattern entries (`NAXISn`, `CDi_j`).
pub fn lookup(keyword: &str) -> Option<&'static KeywordInfo> {
    if let Some(info) = KEYWORDS.iter().find(|i| i.keyword == keyword) {
        return Some(info);
    }
    let pattern = indexed_pattern(keyword)?;
    KEYWORDS.iter().find(|i| i.keyword == pattern)
}

fn indexed_pattern(keyword: &str) -> Option<String> {
    let base = keyword.trim_end_matches(|c: char| c.is_ascii_digit());
    if base.len() == keyword.len() || base.is_empty() {
        return None;
    }
    if let Some(head) = base.strip_suffix('_') {
        let head = head.trim_end_matches(|c: char| c.is_ascii_digit());
        if head.len() < base.len() - 1 {
            return Some(format!("{head}i_j"));
        }
    }
    Some(format!("{base}n"))
}

/// True when the keyword may never be edited (structure and END).
pub fn is_structural(keyword: &str) -> bool {
    keyword == "END" || lookup(keyword).is_some_and(|i| i.structural)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexed() {
        assert_eq!(lookup("NAXIS2").unwrap().keyword, "NAXISn");
        assert_eq!(lookup("CD1_2").unwrap().keyword, "CDi_j");
        assert_eq!(lookup("CRVAL1").unwrap().group, Group::Astrometry);
        assert!(lookup("NOPE").is_none());
    }

    #[test]
    fn structural() {
        assert!(is_structural("BITPIX"));
        assert!(is_structural("NAXIS3"));
        assert!(!is_structural("OBJECT"));
    }

    #[test]
    fn unique() {
        let mut keys: Vec<_> = KEYWORDS.iter().map(|k| k.keyword).collect();
        keys.sort();
        let n = keys.len();
        keys.dedup();
        assert_eq!(n, keys.len());
    }
}
