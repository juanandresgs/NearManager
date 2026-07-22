//! Reusable geometry for two side-by-side surfaces with operator-controlled sizing.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DualSurfaceSide {
    First,
    Second,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DualSurfaceGeometry {
    pub first_width: u16,
    pub second_width: u16,
    pub pane_height: u16,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DualSurfaceLayout {
    first_width_offset: i32,
    pane_height_offset: i32,
}

impl DualSurfaceLayout {
    pub fn geometry(
        self,
        total_width: u16,
        total_height: u16,
        minimum_width: u16,
        minimum_height: u16,
    ) -> DualSurfaceGeometry {
        let minimum_width = minimum_width.min(total_width / 2);
        let first_width = adjusted_extent(
            total_width / 2,
            self.first_width_offset,
            minimum_width,
            total_width.saturating_sub(minimum_width),
        );
        let minimum_height = minimum_height.min(total_height);
        let pane_height = adjusted_extent(
            total_height,
            self.pane_height_offset,
            minimum_height,
            total_height,
        );
        DualSurfaceGeometry {
            first_width,
            second_width: total_width.saturating_sub(first_width),
            pane_height,
        }
    }

    pub fn resize_columns(&mut self, total_width: u16, columns: isize, minimum_width: u16) {
        let current = self.geometry(total_width, 1, minimum_width, 1).first_width;
        let minimum_width = minimum_width.min(total_width / 2);
        let desired = i32::from(current)
            .saturating_add(i32::try_from(columns).unwrap_or(if columns.is_negative() {
                i32::MIN
            } else {
                i32::MAX
            }))
            .clamp(
                i32::from(minimum_width),
                i32::from(total_width.saturating_sub(minimum_width)),
            );
        let desired = u16::try_from(desired).unwrap_or(total_width);
        self.first_width_offset = i32::from(desired) - i32::from(total_width / 2);
    }

    pub fn resize_rows(&mut self, total_height: u16, rows: isize, minimum_height: u16) {
        let current = self
            .geometry(2, total_height, 1, minimum_height)
            .pane_height;
        let desired = i32::from(current)
            .saturating_add(i32::try_from(rows).unwrap_or(if rows.is_negative() {
                i32::MIN
            } else {
                i32::MAX
            }))
            .clamp(
                i32::from(minimum_height.min(total_height)),
                i32::from(total_height),
            );
        let desired = u16::try_from(desired).unwrap_or(total_height);
        self.pane_height_offset = i32::from(desired) - i32::from(total_height);
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn side_at(
        self,
        column: u16,
        total_width: u16,
        minimum_width: u16,
    ) -> Option<DualSurfaceSide> {
        if column >= total_width {
            return None;
        }
        let first_width = self.geometry(total_width, 1, minimum_width, 1).first_width;
        Some(if column < first_width {
            DualSurfaceSide::First
        } else {
            DualSurfaceSide::Second
        })
    }
}

fn adjusted_extent(base: u16, offset: i32, minimum: u16, maximum: u16) -> u16 {
    let adjusted = i32::from(base).saturating_add(offset);
    let maximum = maximum.max(minimum);
    u16::try_from(adjusted.clamp(i32::from(minimum), i32::from(maximum))).unwrap_or(maximum)
}

#[cfg(test)]
mod tests {
    use super::{DualSurfaceLayout, DualSurfaceSide};

    #[test]
    fn resizing_and_hit_testing_share_the_same_geometry() {
        let mut layout = DualSurfaceLayout::default();
        assert_eq!(layout.geometry(100, 30, 8, 5).first_width, 50);
        layout.resize_columns(100, 10, 8);
        let geometry = layout.geometry(100, 30, 8, 5);
        assert_eq!((geometry.first_width, geometry.second_width), (60, 40));
        assert_eq!(layout.side_at(59, 100, 8), Some(DualSurfaceSide::First));
        assert_eq!(layout.side_at(60, 100, 8), Some(DualSurfaceSide::Second));

        layout.resize_rows(30, -5, 5);
        assert_eq!(layout.geometry(100, 30, 8, 5).pane_height, 25);
        layout.resize_rows(30, 5, 5);
        assert_eq!(layout.geometry(100, 30, 8, 5).pane_height, 30);
        layout.reset();
        assert_eq!(layout.geometry(100, 30, 8, 5).first_width, 50);
    }
}
