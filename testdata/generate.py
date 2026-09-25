# /// script
# requires-python = ">=3.10"
# dependencies = ["astropy>=6", "numpy>=1.26"]
# ///
"""Generate the synthetic FITS corpus in testdata/synthetic/.

Written with astropy so Fittle's parser is checked against an independent
writer. Headers imitate each capture app's style but are NOT real captures;
replace or supplement them with real contributed files under testdata/real/.

    uv run testdata/generate.py

Output is deterministic (fixed seed, fixed DATE), so re-running only changes
files when this script changes.
"""

from pathlib import Path

import numpy as np
from astropy.io import fits

OUT = Path(__file__).parent / "synthetic"
W, H = 64, 48
rng = np.random.default_rng(6995)


def sky(bg=1100.0, noise=30.0, stars=12, peak=9000.0, shape=(H, W)):
    img = rng.normal(bg, noise, shape)
    yy, xx = np.mgrid[: shape[0], : shape[1]]
    for _ in range(stars):
        x, y = rng.uniform(4, shape[1] - 4), rng.uniform(4, shape[0] - 4)
        amp, sigma = rng.uniform(0.1, 1.0) * peak, rng.uniform(0.8, 1.6)
        img += amp * np.exp(-((xx - x) ** 2 + (yy - y) ** 2) / (2 * sigma**2))
    return img


def u16(img):
    return np.clip(img, 0, 65535).astype(np.uint16)


def write(rel, hdus):
    path = OUT / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    hdul = fits.HDUList(hdus)
    # Keep output byte-stable: astropy never adds a DATE card unless asked.
    hdul.writeto(path, overwrite=True, output_verify="silentfix")
    return path


def header(cards):
    h = fits.Header()
    for c in cards:
        if isinstance(c, tuple) and c[0] in ("HISTORY", "COMMENT"):
            h.add_history(c[1]) if c[0] == "HISTORY" else h.add_comment(c[1])
        else:
            h.append(c, end=True)
    return h


def seestar_light():
    h = header([
        ("EXTEND", True),
        ("BAYERPAT", "GRBG", "Bayer color pattern"),
        ("XBAYROFF", 0), ("YBAYROFF", 0),
        ("INSTRUME", "Seestar S50", "instrument used"),
        ("TELESCOP", "Seestar S50"),
        ("CREATOR", "ZWO Seestar S50"),
        ("OBJECT", "NGC 6995", "object name"),
        ("RA", 314.2950, "[deg] RA of image center"),
        ("DEC", 31.2353, "[deg] Dec of image center"),
        ("DATE-OBS", "2026-09-25T04:34:12.000", "UTC start of exposure"),
        ("EXPTIME", 20.0, "[s] exposure time"),
        ("EXPOSURE", 20.0),
        ("IMAGETYP", "Light"),
        ("GAIN", 80),
        ("CCD-TEMP", 28.4, "[C] sensor temperature"),
        ("XPIXSZ", 2.9, "[um] pixel width"), ("YPIXSZ", 2.9),
        ("XBINNING", 1), ("YBINNING", 1),
        ("FOCALLEN", 0, "[mm] focal length"),
        ("FILTER", "LP"),
        ("SITELAT", 33.69), ("SITELONG", -117.27),
    ])
    write("seestar/Light_NGC 6995_20.0s_LP_20260924-213412.fit",
          [fits.PrimaryHDU(u16(sky()), h)])


def asiair_light():
    h = header([
        ("EXTEND", True),
        ("INSTRUME", "ZWO ASI2600MC Pro"),
        ("TELESCOP", "RedCat 51"),
        ("SWCREATE", "ASIAIR"),
        ("OBJECT", "M 31"),
        ("OBJCTRA", "00 42 44.3"), ("OBJCTDEC", "+41 16 09"),
        ("DATE-OBS", "2026-09-01T22:15:00.000"),
        ("EXPTIME", 300.0),
        ("IMAGETYP", "Light"),
        ("GAIN", 100), ("OFFSET", 50),
        ("CCD-TEMP", -10.1), ("SET-TEMP", -10.0),
        ("XPIXSZ", 3.76), ("YPIXSZ", 3.76),
        ("XBINNING", 1), ("YBINNING", 1),
        ("FOCALLEN", 250.0),
        ("BAYERPAT", "RGGB"),
        ("FILTER", "L-eXtreme"),
    ])
    write("asiair/Light_M 31_300.0s_Bin1_2600MC_gain100_20260901-221500_-10.0C_0001.fit",
          [fits.PrimaryHDU(u16(sky(bg=900)), h)])


def nina_light():
    h = header([
        ("IMAGETYP", "LIGHT", "Type of exposure"),
        ("EXPOSURE", 180.0, "[s] Exposure duration"),
        ("EXPTIME", 180.0, "[s] Exposure duration"),
        ("DATE-LOC", "2026-08-14T23:02:11.123", "Time of observation (local)"),
        ("DATE-OBS", "2026-08-15T06:02:11.123", "Time of observation (UTC)"),
        ("XBINNING", 1), ("YBINNING", 1),
        ("GAIN", 139), ("OFFSET", 21),
        ("EGAIN", 0.25, "[e-/ADU] Electrons per A/D unit"),
        ("XPIXSZ", 3.76), ("YPIXSZ", 3.76),
        ("INSTRUME", "ZWO ASI294MM Pro"),
        ("SET-TEMP", -10.0), ("CCD-TEMP", -9.8),
        ("TELESCOP", "Esprit 100ED"),
        ("FOCALLEN", 550.0), ("FOCRATIO", 5.5),
        ("RA", 83.8221), ("DEC", -5.3911),
        ("CENTALT", 38.2), ("CENTAZ", 171.4), ("AIRMASS", 1.61),
        ("PIERSIDE", "West"),
        ("SITEELEV", 1350.0), ("SITELAT", 36.1), ("SITELONG", -115.2),
        ("FWHEEL", "ZWO EFW"), ("FILTER", "Ha"),
        ("OBJECT", "M 42"),
        ("OBJCTRA", "05 35 17"), ("OBJCTDEC", "-05 23 28"),
        ("FOCNAME", "ZWO EAF"), ("FOCPOS", 12840), ("FOCTEMP", 11.5),
        ("ROWORDER", "TOP-DOWN"),
        ("EQUINOX", 2000.0),
        ("SWCREATE", "N.I.N.A. 3.1.2.9001"),
    ])
    write("nina/2026-08-14_M 42_Ha_180.00s_0007.fits",
          [fits.PrimaryHDU(u16(sky(bg=600)), h)])


def siril_stack():
    img = (sky(bg=0.08, noise=0.004, peak=0.9) ).astype(np.float32)
    h = header([
        ("INSTRUME", "Seestar S50"),
        ("TELESCOP", "Seestar S50"),
        ("OBJECT", "NGC 6995"),
        ("EXPTIME", 20.0),
        ("STACKCNT", 212, "Stack frames"),
        ("LIVETIME", 4240.0, "[s] Exposure time after deadtime correction"),
        ("DATE-OBS", "2026-09-25T04:34:12"),
        ("FILTER", "LP"),
        ("PROGRAM", "Siril v1.4.0"),
        ("HISTORY", "Background extraction (Correction: Subtraction)"),
        ("HISTORY", "Stacking: Average stacking with Winsorized sigma clipping"),
        ("HISTORY", "Siril stack: 212 images, normalisation: additive + scaling"),
        ("HISTORY", "Registration: global star alignment"),
    ])
    write("siril/r_pp_NGC6995_stacked.fit", [fits.PrimaryHDU(img, h)])


def pixinsight_rgb_master():
    cube = np.stack([sky(bg=0.1, noise=0.003, peak=0.8) for _ in range(3)]).astype(np.float32)
    h = header([
        ("IMAGETYP", "Master Light"),
        ("OBJECT", "IC 1805"),
        ("NCOMBINE", 96),
        ("EXPTIME", 300.0),
        ("TELESCOP", "Esprit 100ED"), ("INSTRUME", "ZWO ASI2600MC Pro"),
        ("FOCALLEN", 550.0), ("XPIXSZ", 3.76),
        ("CTYPE1", "RA---TAN"), ("CTYPE2", "DEC--TAN"),
        ("CRVAL1", 38.175), ("CRVAL2", 61.45),
        ("CRPIX1", 32.5), ("CRPIX2", 24.5),
        ("CD1_1", -0.000392), ("CD1_2", 0.0), ("CD2_1", 0.0), ("CD2_2", 0.000392),
        ("SWCREATE", "PixInsight 1.9.3"),
        ("HISTORY", "ImageIntegration.pixelCombination: average"),
        ("HISTORY", "ImageIntegration.pixelRejection: ESD"),
        ("HISTORY", "ImageIntegration.numberOfImages: 96"),
    ])
    write("pixinsight/masterLight_BIN-1_6248x4176_EXPOSURE-300.00s_FILTER-NoFilter_RGB.fits",
          [fits.PrimaryHDU(cube, h)])


def calibration():
    base = [("INSTRUME", "ZWO ASI2600MC Pro"), ("GAIN", 100), ("OFFSET", 50),
            ("CCD-TEMP", -10.0), ("XBINNING", 1), ("SWCREATE", "N.I.N.A. 3.1.2.9001")]
    write("calibration/DARK_300.00s_0001.fits", [fits.PrimaryHDU(
        u16(rng.normal(510, 8, (H, W))),
        header(base + [("IMAGETYP", "DARK"), ("EXPTIME", 300.0)]))])
    write("calibration/BIAS_0.00s_0001.fits", [fits.PrimaryHDU(
        u16(rng.normal(500, 6, (H, W))),
        header(base + [("IMAGETYP", "BIAS"), ("EXPTIME", 0.0)]))])
    yy, xx = np.mgrid[:H, :W]
    vignette = 1 - 0.25 * (((xx - W / 2) / W) ** 2 + ((yy - H / 2) / H) ** 2) * 4
    write("calibration/FLAT_L-eXtreme_1.20s_0001.fits", [fits.PrimaryHDU(
        u16(rng.normal(28000, 150, (H, W)) * vignette),
        header(base + [("IMAGETYP", "FLAT"), ("EXPTIME", 1.2), ("FILTER", "L-eXtreme")]))])
    write("calibration/master_dark_300s_g100_-10C.fit", [fits.PrimaryHDU(
        rng.normal(510, 1.2, (H, W)).astype(np.float32),
        header(base + [("IMAGETYP", "Master Dark"), ("EXPTIME", 300.0), ("NCOMBINE", 40),
                       ("HISTORY", "Siril stack: 40 images, median")]))])


def edge_cases():
    # Long strings (CONTINUE) and HIERARCH keys.
    h = fits.Header()
    h["OBJECT"] = "Sh2-129 Flying Bat and Ou4 Squid"
    h["NOTES"] = ("Imaged over six nights from a Bortle 4 site; clouds on night five. "
                  "Focus touched up twice. Guiding RMS 0.6 arcsec average.")
    h["HIERARCH ESO DET CHIP NAME"] = "CCD-44"
    h["HIERARCH FITTLE TEST LONGKEY"] = 42
    h.add_comment("Synthetic file exercising CONTINUE and HIERARCH conventions.")
    write("edge/long-strings-hierarch.fits", [fits.PrimaryHDU(u16(sky(stars=3)), h)])

    # Multi-HDU: empty primary, image extension, binary table.
    cols = fits.ColDefs([
        fits.Column(name="X", format="E", array=np.array([12.5, 40.1], dtype=np.float32)),
        fits.Column(name="Y", format="E", array=np.array([8.0, 30.2], dtype=np.float32)),
        fits.Column(name="HFR", format="E", unit="px", array=np.array([2.1, 2.3], dtype=np.float32)),
    ])
    write("edge/multi-hdu.fits", [
        fits.PrimaryHDU(header=header([("OBJECT", "M 27"), ("SWCREATE", "fittle corpus")])),
        fits.ImageHDU(u16(sky()), name="SCI"),
        fits.BinTableHDU.from_columns(cols, name="STARS"),
    ])

    # Rice tile-compressed image (.fz), plus the same pixels uncompressed so
    # decoders can be checked for a lossless round trip.
    img = u16(sky())
    cards = [("OBJECT", "NGC 7000"), ("EXPTIME", 60.0)]
    write("edge/compressed-rice.fits.fz", [
        fits.PrimaryHDU(),
        fits.CompImageHDU(img, header=header(cards), compression_type="RICE_1"),
    ])
    write("edge/compressed-rice-original.fits", [fits.PrimaryHDU(img, header(cards))])

    # RICE_1 variants: pixel widths, 2-D tiles, flat tiles (low entropy) and
    # full-range noise (high entropy). Each has an uncompressed twin.
    variants = {
        "u8": np.clip(sky(bg=40, noise=5, peak=200), 0, 255).astype(np.uint8),
        "i16": np.clip(sky(bg=-2000, noise=30), -32768, 32767).astype(np.int16),
        "i32": (sky(bg=100000, noise=500, peak=2e6)).astype(np.int32),
        "flat": np.full((H, W), 1234, dtype=np.int16),
        "noise": rng.integers(-32768, 32767, (H, W), dtype=np.int16),
    }
    for name, img in variants.items():
        tile = (16, 16) if name in ("i16", "noise") else None
        write(f"edge/rice/{name}.fits.fz", [
            fits.PrimaryHDU(),
            fits.CompImageHDU(img, compression_type="RICE_1", tile_shape=tile),
        ])
        write(f"edge/rice/{name}-original.fits", [fits.PrimaryHDU(img)])

    # Duplicate keywords.
    h = header([("OBJECT", "M 101"), ("GAIN", 100), ("EXPTIME", 120.0)])
    h.append(("GAIN", 120), end=True)
    write("malformed/duplicate-keys.fit", [fits.PrimaryHDU(u16(sky()), h)])

    # Non-standard records, patched in after writing: unquoted string,
    # lowercase keyword, and a non-ASCII byte.
    p = write("malformed/nonstandard-cards.fit", [fits.PrimaryHDU(u16(sky()), header([
        ("OBJECT", "placeholder"), ("FILTER", "placeholder"), ("OBSERVER", "placeholder")]))])
    raw = bytearray(p.read_bytes())
    patch_card(raw, b"OBJECT", b"OBJECT  = M 33 / unquoted string")
    patch_card(raw, b"FILTER", b"filter  = 'Ha'")
    patch_card(raw, b"OBSERVER", "OBSERVER= 'Ren\xe9e'".encode("latin-1"))
    p.write_bytes(raw)

    # Truncated: data unit cut short.
    src = (OUT / "seestar/Light_NGC 6995_20.0s_LP_20260924-213412.fit").read_bytes()
    (OUT / "malformed/truncated.fit").write_bytes(src[: len(src) - 4000])


def documented():
    """Headers modelled on published dumps/source for apps we have no real
    samples of (see crates/fittle-core/data/apps.json `source` fields)."""
    rgb = lambda: np.stack([u16(sky()) for _ in range(3)])
    # Dwarf 3 tele sub and stack (starbash doc/fits/dwarf3).
    base = [("TELESCOP", "DWARFIII"), ("INSTRUME", "DWARFIII"), ("ORIGIN", "DWARFLAB"), ("CAMERA", "TELE"),
            ("OBJECT", "M 31"), ("RA", 10.6847), ("DEC", 41.2687), ("FOCALLEN", 150.0),
            ("XPIXSZ", 2.0), ("YPIXSZ", 2.0), ("XBINNING", 1), ("YBINNING", 1), ("GAIN", 60),
            ("FILTER", "Astro"), ("DET-TEMP", 12), ("BAYERPAT", "RGGB")]
    write("dwarf3/DWARF_RAW_TELE_M 31_EXP_60_GAIN_60_2025-10-20-21-30-00-000/M 31_60s60_Astro_20251020-213512345_12C.fits",
          [fits.PrimaryHDU(u16(sky()), header(base + [("EXPTIME", 60.0), ("DATE-OBS", "2025-10-20T21:35:12.345")]))])
    write("dwarf3/DWARF_RAW_TELE_M 31_EXP_60_GAIN_60_2025-10-20-21-30-00-000/stacked-16_M 31_60s60_Astro_20251020-223000000.fits",
          [fits.PrimaryHDU(rgb(), header(base + [("EXPTIME", 2580.0), ("DATE-OBS", "2025-10-20T22:30:00.000")]))])
    # Unistellar eVscope 2 (fixUnistellarHeaders.py, evtools).
    write("unistellar/eVscope-20251003-221500.fits", [fits.PrimaryHDU(u16(sky()), header([
        ("ORIGIN", "Unistellar"), ("TELESCOP", "eVscope v2.0"), ("INSTRUME", "IMX347"), ("SOFTVER", "4.2"),
        ("OBJECT", "M 57"), ("FOVRA", 283.396), ("FOVDEC", 33.029), ("CMOSTEMP", 21.5),
        ("EXPTIME", 4.0), ("GAIN", 25), ("DATE-OBS", "2025-10-03T22:15:00"),
        ("LATITUDE", 37.77), ("LONGITUD", -122.42), ("ALTITUDE", 30.0), ("BAYERPAT", "GRBG")]))])
    # KStars/Ekos via INDI (indiccd.cpp).
    h = header([("INSTRUME", "ZWO CCD ASI294MC Pro"), ("TELESCOP", "EQMod Mount"), ("OBSERVER", "Unknown"),
                ("OBJECT", "NGC 7000"), ("ROWORDER", "TOP-DOWN"), ("EXPTIME", 120.0), ("DARKTIME", 120.0),
                ("CCD-TEMP", -10.0), ("PIXSIZE1", 4.63), ("PIXSIZE2", 4.63), ("XBINNING", 1), ("YBINNING", 1),
                ("XPIXSZ", 4.63), ("YPIXSZ", 4.63), ("FRAME", "Light"), ("IMAGETYP", "Light Frame"),
                ("GAIN", 120), ("OFFSET", 30), ("FOCALLEN", 400.0), ("APTDIA", 80.0), ("SCALE", 2.388),
                ("SITELAT", 43.6), ("SITELONG", -79.4), ("OBJCTRA", "20 59 17"), ("OBJCTDEC", "44 31 44"),
                ("RA", 314.82), ("DEC", 44.53), ("PIERSIDE", "WEST"), ("DATE-OBS", "2025-08-01T03:10:00.000"),
                ("BAYERPAT", "RGGB")])
    h.add_comment("Generated by INDI")
    write("ekos/Light_NGC_7000_120_secs_2025-08-01T03-10-00_001.fits", [fits.PrimaryHDU(u16(sky()), h)])
    # SharpCap.
    write("sharpcap/Capture_00001.fits", [fits.PrimaryHDU(u16(sky()), header([
        ("SWCREATE", "SharpCap v4.1.12345.0, 64 bit"), ("INSTRUME", "ZWO ASI533MC Pro"), ("EXPTIME", 30.0),
        ("GAIN", 100), ("ROWORDER", "TOP-DOWN"), ("BAYERPAT", "RGGB"), ("XPIXSZ", 3.76),
        ("DATE-OBS", "2025-09-10T02:11:12.1234567")]))])
    # MaxIm DL with sexagesimal site and SNAPSHOT count.
    write("maxim/M13-001-L.fit", [fits.PrimaryHDU(u16(sky()), header([
        ("SWCREATE", "MaxIm DL Version 6.40"), ("IMAGETYP", "Light Frame"), ("OBJECT", "M13"),
        ("OBJCTRA", "16 41 41"), ("OBJCTDEC", "+36 27 36"), ("SITELAT", "+40 00 00"), ("SITELONG", "-105 16 00"),
        ("EXPTIME", 300.0), ("SNAPSHOT", 1), ("FOCALLEN", 1000.0), ("APTDIA", 200.0), ("XPIXSZ", 5.4),
        ("CCD-TEMP", -20.0), ("FILTER", "Lum"), ("DATE-OBS", "2025-06-15T05:00:00")]))])
    # DeepSkyStacker autosave (EXPTIME is the total).
    write("deepskystacker/Autosave.fit", [fits.PrimaryHDU(rgb(), header([
        ("SOFTWARE", "DeepSkyStacker 5.1.6"), ("NCOMBINE", 40), ("EXPTIME", 12000), ("EXPOSURE", 12000),
        ("ROWORDER", "TOP-DOWN"), ("ISOSPEED", 800)]))])
    # ASTAP stack (EXPTIME total, LUM_EXP per sub).
    h = header([("EXPTIME", 3600.0), ("LUM_EXP", 120.0), ("LUM_CNT", 30), ("DARK_CNT", 20), ("FLAT_CNT", 25),
                ("OBJECT", "M 51"), ("CALSTAT", "DF"), ("DATE-OBS", "2025-04-02T21:00:00")])
    h.add_comment("1  Written by Astrometric Stacking Program. www.hnsky.org")
    h.add_history("1  Stacking method SIGMA CLIP AVERAGE")
    write("astap/stack_M51_30x120s.fit", [fits.PrimaryHDU(u16(sky()), h)])
    # ASIAIR master bias (still 16-bit, STACKCNT).
    write("asiair/MasterBias_1.0ms_-9.8C_gain100_2600MC_20250901-120000.fit", [fits.PrimaryHDU(
        u16(rng.normal(500, 1.5, (H, W))), header([
            ("CREATOR", "ZWO ASIAIR Plus"), ("INSTRUME", "ZWO ASI2600MC Duo"), ("TELESCOP", "OnStep"),
            ("IMAGETYP", "Bias"), ("STACKCNT", 100), ("EXPTIME", 0.001), ("GAIN", 100), ("OFFSET", 50),
            ("CCD-TEMP", -9.8)]))])


def checksummed():
    """CHECKSUM/DATASUM written by astropy (an independent implementation).
    Generated last so earlier files keep their random draws."""
    p = OUT / "edge/checksum.fits"
    p.parent.mkdir(parents=True, exist_ok=True)
    fits.HDUList([fits.PrimaryHDU(u16(sky()), header([("OBJECT", "M 57"), ("EXPTIME", 30.0)]))]).writeto(
        p, overwrite=True, checksum=True)


def patch_card(raw, key, record):
    key = key.ljust(8)
    for i in range(0, 2880 * 4, 80):
        if raw[i : i + 8] == key:
            raw[i : i + 80] = record.ljust(80)
            return
    raise KeyError(key)


if __name__ == "__main__":
    seestar_light()
    asiair_light()
    nina_light()
    siril_stack()
    pixinsight_rgb_master()
    calibration()
    edge_cases()
    documented()
    checksummed()
    for p in sorted(OUT.rglob("*")):
        if p.is_file():
            print(f"{p.stat().st_size:>8}  {p.relative_to(OUT)}")
