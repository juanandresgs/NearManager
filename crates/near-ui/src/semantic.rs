use near_core::RoleId;
use ratatui::{buffer::Buffer, layout::Rect};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticCell {
    pub symbol: String,
    pub role: RoleId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticSnapshot {
    width: u16,
    height: u16,
    cells: Vec<SemanticCell>,
}

impl SemanticSnapshot {
    pub fn width(&self) -> u16 {
        self.width
    }

    pub fn height(&self) -> u16 {
        self.height
    }

    pub fn cell(&self, column: u16, row: u16) -> Option<&SemanticCell> {
        if column >= self.width || row >= self.height {
            return None;
        }
        self.cells
            .get(usize::from(row) * usize::from(self.width) + usize::from(column))
    }

    pub fn text_lines(&self) -> Vec<String> {
        (0..self.height)
            .map(|row| {
                (0..self.width)
                    .filter_map(|column| self.cell(column, row))
                    .map(|cell| cell.symbol.as_str())
                    .collect()
            })
            .collect()
    }

    pub fn role_lines(&self) -> Vec<String> {
        (0..self.height)
            .map(|row| {
                let mut runs = Vec::new();
                let mut current: Option<(&str, u16)> = None;
                for column in 0..self.width {
                    let role = self
                        .cell(column, row)
                        .map_or("text", |cell| cell.role.as_str());
                    match &mut current {
                        Some((current_role, count)) if *current_role == role => *count += 1,
                        Some((current_role, count)) => {
                            runs.push(format!("{current_role}:{count}"));
                            current = Some((role, 1));
                        }
                        None => current = Some((role, 1)),
                    }
                }
                if let Some((role, count)) = current {
                    runs.push(format!("{role}:{count}"));
                }
                runs.join(" | ")
            })
            .collect()
    }

    pub(crate) fn from_buffer(buffer: &Buffer, roles: &RoleBuffer) -> Self {
        let cells = (0..buffer.area.height)
            .flat_map(|row| {
                (0..buffer.area.width).map(move |column| SemanticCell {
                    symbol: buffer[(column, row)].symbol().to_owned(),
                    role: roles.role(column, row).clone(),
                })
            })
            .collect();
        Self {
            width: buffer.area.width,
            height: buffer.area.height,
            cells,
        }
    }
}

pub(crate) struct RoleBuffer {
    width: u16,
    height: u16,
    roles: Vec<RoleId>,
}

impl RoleBuffer {
    pub(crate) fn new(width: u16, height: u16, default_role: &str) -> Self {
        Self {
            width,
            height,
            roles: vec![RoleId::from(default_role); usize::from(width) * usize::from(height)],
        }
    }

    pub(crate) fn fill(&mut self, area: Rect, role: &str) {
        let right = area.right().min(self.width);
        let bottom = area.bottom().min(self.height);
        for row in area.y.min(self.height)..bottom {
            for column in area.x.min(self.width)..right {
                let index = usize::from(row) * usize::from(self.width) + usize::from(column);
                self.roles[index] = RoleId::from(role);
            }
        }
    }

    fn role(&self, column: u16, row: u16) -> &RoleId {
        &self.roles[usize::from(row) * usize::from(self.width) + usize::from(column)]
    }
}
