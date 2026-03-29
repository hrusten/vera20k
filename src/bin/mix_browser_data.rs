//! Shared data structures and helpers for the mix-browser tool.

use yrvera_20k::assets::asset_manager::AssetManager;
use yrvera_20k::assets::mix_hash::mix_hash;
use yrvera_20k::assets::shp_file::ShpFile;
use yrvera_20k::assets::xcc_database::XccDatabase;

const HASH_ALIASES: &[(i32, &str)] = &[(0x7AEBAE6Bu32 as i32, "unknown_top_housing.shp")];

/// A single parsed entry from a MIX archive, ready for display.
pub struct MixEntryRow {
    pub index: usize,
    pub hash: i32,
    pub size: u32,
    pub name: String,
    pub identified: bool,
    pub file_type: String,
}

/// Parsed contents of a single MIX archive.
pub struct MixContents {
    pub mix_name: String,
    pub archive_size: usize,
    pub entries: Vec<MixEntryRow>,
    pub identified_count: usize,
    pub error: Option<String>,
}

/// One parseable SHP entry found while scanning all archives.
pub struct ShpEntryRow {
    pub source: String,
    pub index: usize,
    pub hash: i32,
    pub size: u32,
    pub name: String,
    pub identified: bool,
    pub width: u16,
    pub height: u16,
    pub frames: usize,
}

/// Result of scanning all loaded archives for SHP entries.
pub struct ShpIndex {
    pub rows: Vec<ShpEntryRow>,
    pub scanned_archives: usize,
    pub scanned_entries: usize,
}

/// Build the best available hash dictionary for reverse-looking up MIX entries.
///
/// Tries to load the XCC global mix database (~24,000 filenames) first.
/// Falls back to the hardcoded dictionary (~500 names) if XCC is unavailable.
/// Returns `(dictionary, xcc_loaded)`.
pub fn build_best_dictionary() -> (Vec<(String, i32)>, bool) {
    match XccDatabase::load_from_disk() {
        Ok(xcc) => {
            log::info!("Using XCC global mix database ({} entries)", xcc.len(),);
            let mut dict = xcc.build_hash_dictionary();
            // Merge hardcoded names too — they include generated patterns
            // (e.g., tab00..tab19, side1..side9) that XCC may not have.
            let hardcoded = build_hash_dictionary();
            dict.extend(hardcoded);
            dict.sort_by_key(|(_, hash)| *hash);
            dict.dedup_by_key(|(_, hash)| *hash);
            log::info!("Combined dictionary: {} unique hashes", dict.len());
            (dict, true)
        }
        Err(err) => {
            log::info!(
                "XCC database not available ({}), using hardcoded dictionary",
                err
            );
            (build_hash_dictionary(), false)
        }
    }
}

/// Build a large dictionary of plausible RA2/YR filenames to reverse-lookup
/// MIX entry hashes. Returns (filename, hash) pairs sorted by hash.
///
/// This is the fallback when the XCC database is not available.
pub fn build_hash_dictionary() -> Vec<(String, i32)> {
    let mut names: Vec<String> = Vec::new();

    for n in [
        "radar.shp",
        "radary.shp",
        "side1.shp",
        "side2.shp",
        "side3.shp",
        "side2a.shp",
        "side2b.shp",
        "side3a.shp",
        "side3b.shp",
        "tabs.shp",
        "repair.shp",
        "sell.shp",
        "power.shp",
        "credits.shp",
        "clock.shp",
        "pipbrd.shp",
        "pips.shp",
        "pips2.shp",
        "place.shp",
        "sidebar.pal",
        "uibkgd.pal",
        "uibkgdy.pal",
        "radaryuri.pal",
        "cameo.pal",
        "cameomd.pal",
        "mousepal.pal",
        "anim.pal",
    ] {
        names.push(n.to_string());
    }

    for i in 0..20 {
        names.push(format!("tab{i:02}.shp"));
    }
    for i in 1..=9 {
        names.push(format!("side{i}.shp"));
        names.push(format!("side{i}a.shp"));
        names.push(format!("side{i}b.shp"));
    }

    for name in [
        "sidebar",
        "sidebarp",
        "sidebarmd",
        "chrome",
        "cameo",
        "cameomd",
        "unit",
        "unittem",
        "unitsno",
        "uniturb",
        "unitdes",
        "unitlun",
        "temperat",
        "snow",
        "urban",
        "desert",
        "lunar",
        "newurban",
        "isotem",
        "isosno",
        "isourb",
        "isodes",
        "isolun",
        "isonurb",
        "grftxt",
        "mousepal",
        "anim",
        "lib",
        "theater",
    ] {
        names.push(format!("{name}.pal"));
    }

    let sidebar_names = [
        "sidebar.shp",
        "sidebarbg.shp",
        "sidec.shp",
        "chromeframe.shp",
        "chrome.shp",
        "bttn.shp",
        "button.shp",
        "btn.shp",
        "repair2.shp",
        "sell2.shp",
        "power2.shp",
        "repairon.shp",
        "sellon.shp",
        "poweron.shp",
        "repairoff.shp",
        "selloff.shp",
        "poweroff.shp",
        "pgup.shp",
        "pgdn.shp",
        "up.shp",
        "down.shp",
        "hscroll.shp",
        "vscroll.shp",
        "scroll.shp",
        "scrollup.shp",
        "scrolldn.shp",
        "radarbg.shp",
        "radarfr.shp",
        "radarui.shp",
        "radarframe.shp",
        "radarlogo.shp",
        "strip.shp",
        "cameo.shp",
        "queue.shp",
        "ready.shp",
        "hold.shp",
        "onhold.shp",
        "paused.shp",
        "upgrade.shp",
        "upgrdarw.shp",
        "options.shp",
        "diplomcy.shp",
        "battle.shp",
        "mslogo.shp",
        "dialog.shp",
        "dialogs.shp",
        "menu.shp",
        "menubar.shp",
        "mfill.shp",
        "mbar.shp",
        "mbtn.shp",
        "grdylw.shp",
        "grred.shp",
        "grgrn.shp",
        "grwht.shp",
        "gryel.shp",
        "pbar.shp",
        "pbargrn.shp",
        "pbarred.shp",
        "hbar.shp",
        "hpbar.shp",
        "hpips.shp",
        "pwrbar.shp",
        "pwrbaron.shp",
        "pwrbaroff.shp",
        "tooltip.shp",
        "txtbg.shp",
        "text.shp",
        "version.shp",
        "logo.shp",
        "westwood.shp",
        "title.shp",
        "titlebar.shp",
        "mouse.shp",
        "cursor.shp",
        "pointer.shp",
        "preview.shp",
        "pview.shp",
        "eva.shp",
        "evabg.shp",
        "evabar.shp",
        "select.shp",
        "health.shp",
        "rank.shp",
        "vet.shp",
        "elite.shp",
        "spyplane.shp",
        "paradrop.shp",
        "nuke.shp",
        "lightning.shp",
        "chrono.shp",
        "iron.shp",
        "ironcurt.shp",
        "waypoint.shp",
        "beacon.shp",
        "guard.shp",
        "stop.shp",
        "deploy.shp",
    ];
    for n in sidebar_names {
        names.push(n.to_string());
    }

    let prefixes = [
        "side", "tab", "btn", "bttn", "ctrl", "knob", "bar", "strip", "slot", "cell", "cam",
        "icon", "pip", "tic", "clock", "radar", "pbar", "hbar", "grn", "red", "yel", "wht", "gry",
        "grdylw", "grred", "grgrn", "grwht",
    ];
    for prefix in prefixes {
        for i in 0..30 {
            names.push(format!("{prefix}{i:02}.shp"));
            names.push(format!("{prefix}{i}.shp"));
        }
    }

    for base in [
        "sidebar", "sidebarp", "radar", "side1", "side2", "side3", "tabs", "tab00", "tab01",
        "tab02", "tab03", "repair", "sell", "power", "credits", "strip", "cameo", "chrome",
    ] {
        names.push(format!("{base}md.shp"));
        names.push(format!("{base}.shp"));
        names.push(format!("{base}md.pal"));
        names.push(format!("{base}.pal"));
    }

    let misc = [
        "gafscrn",
        "gafscrnmd",
        "nafscrnmd",
        "nafscreen",
        "yafscrn",
        "yafscrnmd",
        "gascren",
        "nascren",
        "yascren",
        "gaside1",
        "gaside2",
        "gaside3",
        "naside1",
        "naside2",
        "naside3",
        "yaside1",
        "yaside2",
        "yaside3",
        "gatabs",
        "natabs",
        "yatabs",
        "garadar",
        "naradar",
        "yaradar",
        "sldbkgd",
        "sldbar",
        "sldbkg",
        "bkgnd",
        "bkgd",
        "background",
        "pwrup",
        "pwrdn",
        "pwrbar",
        "credbar",
        "credbg",
        "crednum",
        "mnubtns",
        "mnubtn",
        "menubtn",
        "optbtns",
        "optbtn",
        "frame",
        "framebg",
        "framefg",
    ];
    for n in misc {
        names.push(format!("{n}.shp"));
        names.push(format!("{n}.pal"));
    }

    let common_assets = [
        "gapowr", "gacnst", "garefn", "gawall", "gagate", "gapile", "gaweap", "gaairc", "gadept",
        "gatech", "garobo", "gaspysat", "gaorep", "napowr", "nacnst", "narefn", "nawall", "nagate",
        "nahand", "naweap", "nayard", "natech", "naflak", "nalasr", "naradr", "yapowr", "yacnst",
        "yarefn", "yawall", "yagate", "yabrck", "yaweap", "yayard", "yatech", "yagrnd", "amcv",
        "smcv", "pcv", "harv", "sref", "htnk", "mtnk", "ltnk", "fv", "ifv", "bfrt", "apoc", "deso",
        "dred", "aegis", "dest", "howi", "v3", "flak", "rhino", "grizzly", "mirage", "prism", "e1",
        "e2", "e3", "e4", "dog", "snipe", "spy", "engi", "chrono", "seal", "boris", "tanya",
        "ivan", "yuri", "init",
    ];
    for n in common_assets {
        names.push(format!("{n}.shp"));
        names.push(format!("{n}.vxl"));
        names.push(format!("{n}icon.shp"));
        names.push(format!("{n}uiicon.shp"));
    }

    let mut dict: Vec<(String, i32)> = names
        .into_iter()
        .map(|n| {
            let hash = mix_hash(&n);
            (n, hash)
        })
        .collect();
    dict.sort_by_key(|(_, hash)| *hash);
    dict.dedup_by_key(|(_, hash)| *hash);
    dict
}

/// Expand the hash dictionary with filenames extracted from rules.ini and art.ini.
///
/// Scans all `Image=` and `Cameo=` values and adds `.shp`, `.vxl`, and `icon.shp`
/// variants. This identifies hundreds of previously-unknown archive entries.
pub fn expand_dictionary_from_ini(dict: &mut Vec<(String, i32)>, asset_manager: &AssetManager) {
    let mut extra_names: Vec<String> = Vec::new();

    // Load rules.ini + rulesmd.ini + art.ini + artmd.ini as raw text.
    for ini_name in ["rules.ini", "rulesmd.ini", "art.ini", "artmd.ini"] {
        let Some(data) = asset_manager.get(ini_name) else {
            continue;
        };
        let text = String::from_utf8_lossy(&data);
        for line in text.lines() {
            let trimmed = line.trim();
            // Skip comments and section headers.
            if trimmed.starts_with(';') || trimmed.starts_with('[') || !trimmed.contains('=') {
                continue;
            }
            let Some((key, value)) = trimmed.split_once('=') else {
                continue;
            };
            let key_upper = key.trim().to_ascii_uppercase();
            let val = value.trim();
            if val.is_empty() {
                continue;
            }

            match key_upper.as_str() {
                "IMAGE" | "CAMEO" | "ALTCAMEO" => {
                    let lower = val.to_ascii_lowercase();
                    extra_names.push(format!("{}.shp", lower));
                    extra_names.push(format!("{}.vxl", lower));
                    extra_names.push(format!("{}icon.shp", lower));
                    extra_names.push(format!("{}.hva", lower));
                }
                _ => {}
            }
        }
    }

    if extra_names.is_empty() {
        return;
    }

    let new_entries: Vec<(String, i32)> = extra_names
        .into_iter()
        .map(|n| {
            let hash = mix_hash(&n);
            (n, hash)
        })
        .collect();

    dict.extend(new_entries);
    dict.sort_by_key(|(_, hash)| *hash);
    dict.dedup_by_key(|(_, hash)| *hash);
    log::info!(
        "Hash dictionary expanded to {} entries from INI files",
        dict.len()
    );
}

/// CSF string table magic: " FSC" in little-endian = 0x46435320.
const CSF_MAGIC: [u8; 4] = [0x20, 0x53, 0x43, 0x46];

/// Detect a rough file type from header bytes.
pub fn detect_file_type(data: &[u8]) -> String {
    if data.len() < 4 {
        return format!("tiny ({} bytes)", data.len());
    }

    // PAL: exactly 768 bytes, all values <= 63 (VGA 6-bit).
    if data.len() == 768 {
        let all_vga = data.iter().all(|&b| b <= 63);
        if all_vga {
            return "PAL (VGA 6-bit, 256 colors)".to_string();
        }
        return "PAL (768 bytes)".to_string();
    }

    // VXL: 16-byte magic "Voxel Animation\0".
    if data.len() >= 34 && &data[0..16] == b"Voxel Animation\0" {
        let limb_count = u32::from_le_bytes([data[28], data[29], data[30], data[31]]);
        return format!("VXL (Voxel Animation, {} limbs)", limb_count);
    }

    // HVA: 16-byte ASCII filename, then u32 frame_count, u32 limb_count.
    if data.len() >= 24 {
        let header_ascii = data[0..16]
            .iter()
            .all(|&b| b == 0 || (0x20..=0x7E).contains(&b));
        if header_ascii {
            let frames = u32::from_le_bytes([data[16], data[17], data[18], data[19]]);
            let limbs = u32::from_le_bytes([data[20], data[21], data[22], data[23]]);
            // Reasonable bounds: 1-999 frames, 1-50 limbs.
            if frames >= 1 && frames < 1000 && limbs >= 1 && limbs < 50 {
                let expected_size = 24 + limbs * 16 + frames * limbs * 48;
                // Allow some tolerance (file may have padding).
                if data.len() as u32 >= expected_size {
                    let name: String = data[0..16]
                        .iter()
                        .take_while(|&&b| b != 0)
                        .map(|&b| b as char)
                        .collect();
                    return format!("HVA ({} frames, {} limbs, \"{}\")", frames, limbs, name);
                }
            }
        }
    }

    // CSF: magic " FSC" (0x20534346).
    if data.len() >= 24 && data[0..4] == CSF_MAGIC {
        let num_labels = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
        return format!("CSF (string table, {} labels)", num_labels);
    }

    // TMP: header is template_w(u32) template_h(u32) tile_w(u32) tile_h(u32).
    if data.len() >= 16 {
        let tw = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let th = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        let tile_w = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
        let tile_h = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
        // RA2 tiles are 60x30; template dims are small (1-20).
        if tw >= 1 && tw <= 20 && th >= 1 && th <= 20 && tile_w == 60 && tile_h == 30 {
            return format!("TMP ({}x{} template, {}x{} tiles)", tw, th, tile_w, tile_h);
        }
    }

    // AUD: sample_rate(u16) size(u32) flags(u8) compression(u8).
    if data.len() >= 12 {
        let sample_rate = u16::from_le_bytes([data[0], data[1]]);
        let compression = data[9];
        // Westwood IMA ADPCM = 99, uncompressed = 1.
        if sample_rate >= 8000 && sample_rate <= 48000 && (compression == 1 || compression == 99) {
            let comp_name = if compression == 99 {
                "IMA ADPCM"
            } else {
                "PCM"
            };
            return format!("AUD ({} Hz, {})", sample_rate, comp_name);
        }
    }

    // SHP(TS): first u16 = 0, then width, height, frame_count.
    let w0 = u16::from_le_bytes([data[0], data[1]]);
    if w0 == 0 && data.len() >= 8 {
        let width = u16::from_le_bytes([data[2], data[3]]);
        let height = u16::from_le_bytes([data[4], data[5]]);
        let frame_count = u16::from_le_bytes([data[6], data[7]]);
        if frame_count > 0
            && frame_count < 5000
            && width > 0
            && width < 4096
            && height > 0
            && height < 4096
        {
            return format!("SHP(TS) {}x{} {} frames", width, height, frame_count);
        }
    }

    // SHP(old): first u16 is frame count, then first offset.
    if w0 > 0 && w0 < 200 && data.len() >= 14 {
        let first_offset = u32::from_le_bytes([data[2], data[3], data[4], data[5]]);
        if first_offset > 0 && first_offset < data.len() as u32 {
            return format!("SHP(old?) {} frames, off0={}", w0, first_offset);
        }
    }

    if data.len() > 52 && &data[0..4] == b"XCC " {
        return "XCC database".to_string();
    }

    // Mostly-ASCII = probably text/INI.
    let ascii_count = data
        .iter()
        .take(64)
        .filter(|&&b| (0x20..=0x7E).contains(&b))
        .count();
    if ascii_count > 48 {
        let preview: String = data
            .iter()
            .take(40)
            .map(|&b| {
                if (0x20..=0x7E).contains(&b) {
                    b as char
                } else {
                    '.'
                }
            })
            .collect();
        return format!("text \"{}...\"", preview);
    }

    format!(
        "binary ({}B, hdr={:02X}{:02X}{:02X}{:02X})",
        data.len(),
        data[0],
        data[1],
        data[2],
        data[3]
    )
}

/// Parse one archive and build display-ready rows.
pub fn load_mix_contents(
    asset_manager: &AssetManager,
    mix_name: &str,
    dict: &[(String, i32)],
) -> MixContents {
    let Some(archive) = asset_manager.archive(mix_name) else {
        return MixContents {
            mix_name: mix_name.to_string(),
            archive_size: 0,
            entries: Vec::new(),
            identified_count: 0,
            error: Some("Archive not loaded".to_string()),
        };
    };

    let mut rows = Vec::with_capacity(archive.entry_count());
    let mut identified_count = 0usize;

    for (i, entry) in archive.entries().iter().enumerate() {
        let (name, identified) = resolve_name(dict, entry.id);
        if identified {
            identified_count += 1;
        }

        let file_type = archive
            .get_by_id(entry.id)
            .map(detect_file_type)
            .unwrap_or_else(|| "read error".to_string());

        rows.push(MixEntryRow {
            index: i,
            hash: entry.id,
            size: entry.size,
            name,
            identified,
            file_type,
        });
    }

    MixContents {
        mix_name: mix_name.to_string(),
        archive_size: archive.byte_len(),
        entries: rows,
        identified_count,
        error: None,
    }
}

/// Scan all loaded archives and collect every entry that parses as SHP.
pub fn build_shp_index(asset_manager: &AssetManager, dict: &[(String, i32)]) -> ShpIndex {
    let mut rows = Vec::new();
    let mut scanned_archives = 0usize;
    let mut scanned_entries = 0usize;

    asset_manager.visit_archives(|source, archive| {
        scanned_archives += 1;
        scanned_entries += archive.entry_count();

        for (index, entry) in archive.entries().iter().enumerate() {
            let Some(data) = archive.get_by_id(entry.id) else {
                continue;
            };
            let Ok(shp) = ShpFile::from_bytes(data) else {
                continue;
            };

            let (name, identified) = resolve_name(dict, entry.id);
            rows.push(ShpEntryRow {
                source: source.to_string(),
                index,
                hash: entry.id,
                size: entry.size,
                name,
                identified,
                width: shp.width,
                height: shp.height,
                frames: shp.frames.len(),
            });
        }
    });

    rows.sort_by(|a, b| {
        (!a.identified)
            .cmp(&!b.identified)
            .then_with(|| {
                a.name
                    .to_ascii_lowercase()
                    .cmp(&b.name.to_ascii_lowercase())
            })
            .then_with(|| {
                a.source
                    .to_ascii_lowercase()
                    .cmp(&b.source.to_ascii_lowercase())
            })
            .then_with(|| a.hash.cmp(&b.hash))
    });

    ShpIndex {
        rows,
        scanned_archives,
        scanned_entries,
    }
}

fn resolve_name(dict: &[(String, i32)], hash: i32) -> (String, bool) {
    if let Some((_, alias)) = HASH_ALIASES
        .iter()
        .find(|(alias_hash, _)| *alias_hash == hash)
    {
        return ((*alias).to_string(), true);
    }

    dict.iter()
        .find(|(_, candidate_hash)| *candidate_hash == hash)
        .map(|(name, _)| (name.clone(), true))
        .unwrap_or_else(|| (format!("??? ({:#010X})", hash), false))
}
