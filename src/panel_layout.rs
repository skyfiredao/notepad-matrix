use std::collections::BTreeMap;
use std::io;

use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PanelId {
    DocumentList,
    DocumentMap,
    FunctionList,
    Folder,
    Project1,
    Project2,
    Project3,
    Characters,
}

impl PanelId {
    pub const ALL: [Self; 8] = [
        Self::DocumentList,
        Self::DocumentMap,
        Self::FunctionList,
        Self::Folder,
        Self::Project1,
        Self::Project2,
        Self::Project3,
        Self::Characters,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::DocumentList => "document-list",
            Self::DocumentMap => "document-map",
            Self::FunctionList => "function-list",
            Self::Folder => "folder",
            Self::Project1 => "project-1",
            Self::Project2 => "project-2",
            Self::Project3 => "project-3",
            Self::Characters => "characters",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::DocumentList => "Document List",
            Self::DocumentMap => "Document Map",
            Self::FunctionList => "Function List",
            Self::Folder => "Folder as Workspace",
            Self::Project1 => "Project Panel 1",
            Self::Project2 => "Project Panel 2",
            Self::Project3 => "Project Panel 3",
            Self::Characters => "ASCII Codes Insertion Panel",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockSide {
    Left,
    Right,
    Top,
    Bottom,
}

impl DockSide {
    pub const ALL: [Self; 4] = [Self::Left, Self::Right, Self::Top, Self::Bottom];

    pub fn key(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }

    fn parse(value: &Value) -> io::Result<Self> {
        match value.as_str() {
            Some("left") => Ok(Self::Left),
            Some("right") => Ok(Self::Right),
            Some("top") => Ok(Self::Top),
            Some("bottom") => Ok(Self::Bottom),
            _ => Err(invalid("Invalid dock side")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    Docked(DockSide),
    Floating,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FloatBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Default for FloatBounds {
    fn default() -> Self {
        Self {
            x: 120.,
            y: 120.,
            width: 300.,
            height: 500.,
        }
    }
}

impl FloatBounds {
    fn validate(self) -> io::Result<Self> {
        if [self.x, self.y, self.width, self.height]
            .iter()
            .all(|n| n.is_finite())
            && self.width > 0.
            && self.height > 0.
        {
            Ok(self)
        } else {
            Err(invalid("Invalid floating window bounds"))
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelPlacement {
    pub visible: bool,
    pub placement: Placement,
    pub dock: DockSide,
    pub order: usize,
    pub size: f32,
    pub bounds: FloatBounds,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PanelLayout {
    panels: BTreeMap<PanelId, PanelPlacement>,
}

impl Default for PanelLayout {
    fn default() -> Self {
        Self {
            panels: PanelId::ALL
                .into_iter()
                .enumerate()
                .map(|(order, id)| {
                    let dock = if matches!(
                        id,
                        PanelId::DocumentMap | PanelId::FunctionList | PanelId::Characters
                    ) {
                        DockSide::Right
                    } else {
                        DockSide::Left
                    };
                    let size = match id {
                        PanelId::DocumentList => 180.,
                        PanelId::DocumentMap => 180.,
                        _ => 250.,
                    };
                    (
                        id,
                        PanelPlacement {
                            visible: false,
                            placement: Placement::Docked(dock),
                            dock,
                            order,
                            size,
                            bounds: FloatBounds::default(),
                        },
                    )
                })
                .collect(),
        }
    }
}

impl PanelLayout {
    pub fn panel(&self, id: PanelId) -> PanelPlacement {
        self.panels
            .get(&id)
            .copied()
            .unwrap_or_else(|| PanelPlacement {
                visible: false,
                placement: Placement::Docked(DockSide::Left),
                dock: DockSide::Left,
                order: 0,
                size: 250.,
                bounds: FloatBounds::default(),
            })
    }

    pub fn show(&mut self, id: PanelId) {
        self.update(id, |panel| panel.visible = true);
    }
    pub fn hide(&mut self, id: PanelId) {
        self.update(id, |panel| panel.visible = false);
    }

    pub fn dock(&mut self, id: PanelId, side: DockSide, order: usize) {
        let mut neighbors = self.docked(side);
        neighbors.retain(|other| *other != id);
        neighbors.insert(order.min(neighbors.len()), id);
        for (index, neighbor) in neighbors.into_iter().enumerate() {
            self.update(neighbor, |panel| {
                panel.order = index;
            });
        }
        self.update(id, |panel| {
            panel.dock = side;
            panel.placement = Placement::Docked(side);
        });
    }

    pub fn redock(&mut self, id: PanelId) {
        let panel = self.panel(id);
        self.dock(id, panel.dock, panel.order);
    }

    pub fn float(&mut self, id: PanelId, bounds: FloatBounds) -> io::Result<()> {
        let bounds = bounds.validate()?;
        self.update(id, |panel| {
            panel.placement = Placement::Floating;
            panel.bounds = bounds;
        });
        Ok(())
    }

    pub fn set_bounds(&mut self, id: PanelId, bounds: FloatBounds) -> io::Result<()> {
        let bounds = bounds.validate()?;
        self.update(id, |panel| panel.bounds = bounds);
        Ok(())
    }

    pub fn resize(&mut self, id: PanelId, size: f32) -> io::Result<()> {
        if !size.is_finite() || size <= 0. {
            return Err(invalid("Panel size must be positive"));
        }
        self.update(id, |panel| panel.size = size);
        Ok(())
    }

    pub fn docked(&self, side: DockSide) -> Vec<PanelId> {
        let mut panels: Vec<_> = PanelId::ALL
            .into_iter()
            .filter(|id| {
                let panel = self.panel(*id);
                panel.visible && panel.placement == Placement::Docked(side)
            })
            .collect();
        panels.sort_by_key(|id| (self.panel(*id).order, *id));
        panels
    }

    pub fn to_json(&self) -> Value {
        let panels: serde_json::Map<String, Value> = PanelId::ALL.into_iter().map(|id| {
            let p = self.panel(id);
            (id.key().into(), json!({ "visible": p.visible, "floating": p.placement == Placement::Floating,
                "dock": p.dock.key(), "order": p.order, "size": p.size,
                "bounds": { "x": p.bounds.x, "y": p.bounds.y, "width": p.bounds.width, "height": p.bounds.height } }))
        }).collect();
        json!({ "version": 1, "panels": panels })
    }

    pub fn from_json(value: &Value) -> io::Result<Self> {
        if value.get("version").and_then(Value::as_u64) != Some(1) {
            return Err(invalid("Unsupported panel layout version"));
        }
        let values = value
            .get("panels")
            .and_then(Value::as_object)
            .ok_or_else(|| invalid("Missing panel placements"))?;
        let mut layout = Self::default();
        for id in PanelId::ALL {
            let Some(value) = values.get(id.key()) else {
                continue;
            };
            let dock = DockSide::parse(&value["dock"])?;
            let visible = value["visible"]
                .as_bool()
                .ok_or_else(|| invalid("Invalid panel visibility"))?;
            let floating = value["floating"]
                .as_bool()
                .ok_or_else(|| invalid("Invalid panel placement"))?;
            let order = value["order"]
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .ok_or_else(|| invalid("Invalid panel order"))?;
            let number = |value: &Value| {
                value
                    .as_f64()
                    .map(|n| n as f32)
                    .filter(|n| n.is_finite())
                    .ok_or_else(|| invalid("Invalid panel dimension"))
            };
            let size = number(&value["size"])?;
            if size <= 0. {
                return Err(invalid("Invalid panel size"));
            }
            let b = &value["bounds"];
            let bounds = FloatBounds {
                x: number(&b["x"])?,
                y: number(&b["y"])?,
                width: number(&b["width"])?,
                height: number(&b["height"])?,
            }
            .validate()?;
            layout.panels.insert(
                id,
                PanelPlacement {
                    visible,
                    placement: if floating {
                        Placement::Floating
                    } else {
                        Placement::Docked(dock)
                    },
                    dock,
                    order,
                    size,
                    bounds,
                },
            );
        }
        Ok(layout)
    }

    fn update(&mut self, id: PanelId, update: impl FnOnce(&mut PanelPlacement)) {
        let mut panel = self.panel(id);
        update(&mut panel);
        self.panels.insert(id, panel);
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
