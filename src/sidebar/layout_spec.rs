use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::{
    CAMEO_GAP_X, CAMEO_H, CAMEO_INSET_X, CAMEO_INSET_Y, CAMEO_W, CONTROL_BLOCK_BOTTOM_PAD,
    CONTROL_BLOCK_TOP_PAD, CONTROL_BUTTON_GAP, CONTROL_BUTTON_HEIGHT, RADAR_CONTENT_HEIGHT,
    RADAR_CONTENT_WIDTH, RADAR_HEIGHT, SIDE1_HEIGHT, SIDE2_HEIGHT, SIDE3_HEIGHT, SIDEBAR_TOP_INSET,
    SIDEBAR_WIDTH, TABS_HEIGHT,
};

pub const SIDEBAR_LAYOUT_FILE_NAME: &str = "src/sidebar/sidebar_layout.ron";

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SidebarChromeLayoutSpec {
    pub x_offset: f32,
    pub top_inset: f32,
    pub fill_to_bottom: bool,
    pub fill_bottom_margin: f32,
    pub top_strip_sidebar_x: f32,
    pub top_strip_sidebar_y: f32,
    pub top_strip_thin_x: f32,
    pub top_strip_thin_y: f32,
    pub unknown_top_housing_x: f32,
    pub unknown_top_housing_y: f32,
    pub unknown_top_housing_width: f32,
    pub unknown_top_housing_height: f32,
    pub repair_x: f32,
    pub repair_y: f32,
    pub sell_x: f32,
    pub sell_y: f32,
    pub radar_height: f32,
    pub side1_height: f32,
    pub tabs_height: f32,
    pub side2_height: f32,
    pub side3_height: f32,
    pub cameo_inset_x: f32,
    pub cameo_inset_y: f32,
    pub cameo_gap_x: f32,
    pub cameo_width: f32,
    pub cameo_height: f32,
    pub cameo_row_height: f32,
    /// Power bar x-offset from sidebar left edge (powerp.shp vertical meter).
    pub power_bar_x: f32,
    /// Power bar top inset below tabs_y (top of bar).
    pub power_bar_top_y: f32,
    /// Power bar bottom inset above side3 bottom edge.
    pub power_bar_bottom_y: f32,
    /// Power bar width in pixels.
    pub power_bar_width: f32,
    /// Power bar tile height in pixels (native powerp.shp is 2px).
    pub power_bar_tile_height: f32,
    /// Total sidebar width (all chrome SHPs are this wide).
    pub sidebar_width: f32,
    /// Radar minimap content area dimensions (inside the radar chrome piece).
    pub radar_content_width: f32,
    pub radar_content_height: f32,
    /// Control button dimensions and spacing at bottom of sidebar.
    pub control_button_height: f32,
    pub control_button_gap: f32,
    pub control_block_top_pad: f32,
    pub control_block_bottom_pad: f32,
}

impl SidebarChromeLayoutSpec {
    pub const fn stock() -> Self {
        Self {
            x_offset: 0.0,
            top_inset: SIDEBAR_TOP_INSET,
            fill_to_bottom: false,
            fill_bottom_margin: 0.0,
            top_strip_sidebar_x: 0.0,
            top_strip_sidebar_y: 0.0,
            top_strip_thin_x: 0.0,
            top_strip_thin_y: 32.0,
            unknown_top_housing_x: 0.0,
            unknown_top_housing_y: 0.0,
            unknown_top_housing_width: -1.0,
            unknown_top_housing_height: -1.0,
            repair_x: 8.0,
            repair_y: 20.0,
            sell_x: 96.0,
            sell_y: 20.0,
            power_bar_x: 6.0,
            power_bar_top_y: 4.0,
            power_bar_bottom_y: 4.0,
            power_bar_width: 10.0,
            power_bar_tile_height: 3.0,
            radar_height: RADAR_HEIGHT,
            side1_height: SIDE1_HEIGHT,
            tabs_height: TABS_HEIGHT,
            side2_height: SIDE2_HEIGHT,
            side3_height: SIDE3_HEIGHT,
            cameo_inset_x: CAMEO_INSET_X,
            cameo_inset_y: CAMEO_INSET_Y,
            cameo_gap_x: CAMEO_GAP_X,
            cameo_width: CAMEO_W,
            cameo_height: CAMEO_H,
            cameo_row_height: SIDE2_HEIGHT,
            sidebar_width: SIDEBAR_WIDTH,
            radar_content_width: RADAR_CONTENT_WIDTH,
            radar_content_height: RADAR_CONTENT_HEIGHT,
            control_button_height: CONTROL_BUTTON_HEIGHT,
            control_button_gap: CONTROL_BUTTON_GAP,
            control_block_top_pad: CONTROL_BLOCK_TOP_PAD,
            control_block_bottom_pad: CONTROL_BLOCK_BOTTOM_PAD,
        }
    }

    /// Return a copy with all dimensional fields multiplied by `factor`.
    /// Used for integer UI scaling (2x, 3x) so the sidebar grows uniformly.
    pub fn with_scale(&self, factor: f32) -> Self {
        Self {
            x_offset: self.x_offset * factor,
            top_inset: self.top_inset * factor,
            fill_to_bottom: self.fill_to_bottom,
            fill_bottom_margin: self.fill_bottom_margin * factor,
            top_strip_sidebar_x: self.top_strip_sidebar_x * factor,
            top_strip_sidebar_y: self.top_strip_sidebar_y * factor,
            top_strip_thin_x: self.top_strip_thin_x * factor,
            top_strip_thin_y: self.top_strip_thin_y * factor,
            unknown_top_housing_x: self.unknown_top_housing_x * factor,
            unknown_top_housing_y: self.unknown_top_housing_y * factor,
            unknown_top_housing_width: if self.unknown_top_housing_width > 0.0 {
                self.unknown_top_housing_width * factor
            } else {
                self.unknown_top_housing_width
            },
            unknown_top_housing_height: if self.unknown_top_housing_height > 0.0 {
                self.unknown_top_housing_height * factor
            } else {
                self.unknown_top_housing_height
            },
            repair_x: self.repair_x * factor,
            repair_y: self.repair_y * factor,
            sell_x: self.sell_x * factor,
            sell_y: self.sell_y * factor,
            power_bar_x: self.power_bar_x * factor,
            power_bar_top_y: self.power_bar_top_y * factor,
            power_bar_bottom_y: self.power_bar_bottom_y * factor,
            power_bar_width: self.power_bar_width * factor,
            power_bar_tile_height: self.power_bar_tile_height * factor,
            radar_height: self.radar_height * factor,
            side1_height: self.side1_height * factor,
            tabs_height: self.tabs_height * factor,
            side2_height: self.side2_height * factor,
            side3_height: self.side3_height * factor,
            cameo_inset_x: self.cameo_inset_x * factor,
            cameo_inset_y: self.cameo_inset_y * factor,
            cameo_gap_x: self.cameo_gap_x * factor,
            cameo_width: self.cameo_width * factor,
            cameo_height: self.cameo_height * factor,
            cameo_row_height: self.cameo_row_height * factor,
            sidebar_width: self.sidebar_width * factor,
            radar_content_width: self.radar_content_width * factor,
            radar_content_height: self.radar_content_height * factor,
            control_button_height: self.control_button_height * factor,
            control_button_gap: self.control_button_gap * factor,
            control_block_top_pad: self.control_block_top_pad * factor,
            control_block_bottom_pad: self.control_block_bottom_pad * factor,
        }
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read sidebar layout file: {}", path.display()))?;
        ron::from_str(&raw)
            .with_context(|| format!("Failed to parse sidebar layout file: {}", path.display()))
    }

    pub fn load_optional_from(path: &Path) -> Result<Option<Self>> {
        match path.try_exists() {
            Ok(true) => Self::load_from(path).map(Some),
            Ok(false) => Ok(None),
            Err(err) => Err(err).with_context(|| {
                format!("Failed to check sidebar layout file: {}", path.display())
            }),
        }
    }

    pub fn load_optional_default() -> Result<Option<Self>> {
        Self::load_optional_from(Path::new(SIDEBAR_LAYOUT_FILE_NAME))
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        let pretty = ron::ser::PrettyConfig::new()
            .depth_limit(2)
            .separate_tuple_members(true)
            .enumerate_arrays(true);
        let raw = ron::ser::to_string_pretty(self, pretty)
            .context("Failed to serialize sidebar layout spec")?;
        std::fs::write(path, raw)
            .with_context(|| format!("Failed to write sidebar layout file: {}", path.display()))
    }

    pub fn save_default(&self) -> Result<()> {
        self.save_to(Path::new(SIDEBAR_LAYOUT_FILE_NAME))
    }
}

impl Default for SidebarChromeLayoutSpec {
    fn default() -> Self {
        Self::stock()
    }
}

#[cfg(test)]
mod tests {
    use super::SidebarChromeLayoutSpec;

    #[test]
    fn stock_round_trip_ron() {
        let spec = SidebarChromeLayoutSpec::stock();
        let raw = ron::to_string(&spec).expect("serialize");
        let decoded: SidebarChromeLayoutSpec = ron::from_str(&raw).expect("deserialize");
        assert_eq!(decoded, spec);
    }

    #[test]
    fn with_scale_doubles_dimensions() {
        let base = SidebarChromeLayoutSpec::stock();
        let scaled = base.with_scale(2.0);
        assert_eq!(scaled.sidebar_width, base.sidebar_width * 2.0);
        assert_eq!(scaled.cameo_width, base.cameo_width * 2.0);
        assert_eq!(scaled.radar_height, base.radar_height * 2.0);
        assert_eq!(
            scaled.control_button_height,
            base.control_button_height * 2.0
        );
        // Boolean fields are unchanged.
        assert_eq!(scaled.fill_to_bottom, base.fill_to_bottom);
    }

    #[test]
    fn with_scale_1x_is_identity() {
        let base = SidebarChromeLayoutSpec::stock();
        let scaled = base.with_scale(1.0);
        assert_eq!(scaled, base);
    }
}
