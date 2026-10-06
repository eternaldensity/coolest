//! Pane layout geometry (design/08 single-window spec). Pure data —
//! the egui shell uses these same rects with widgets; the software
//! renderer in `render` fills them with shapes for headless verification.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    pub fn right(&self) -> u32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> u32 {
        self.y + self.h
    }

    pub fn contains(&self, o: &Rect) -> bool {
        self.x <= o.x && self.y <= o.y && self.right() >= o.right() && self.bottom() >= o.bottom()
    }

    pub fn overlaps(&self, o: &Rect) -> bool {
        self.x < o.right() && o.x < self.right() && self.y < o.bottom() && o.y < self.bottom()
    }
}

#[derive(Debug, Clone)]
pub struct Panes {
    pub window: Rect,
    pub console: Rect, // bottom: command input
    pub status: Rect,  // left: time/device/score/health/money
    pub actions: Rect, // centre-top: procs + cpu/trace
    pub notify: Rect,  // top-right: mail/delivery
    pub map: Rect,     // top-right below notify: zoomable map
}

/// 1280x800 single window. Map enlarges when off-device (actions hides);
//  `map_focused` selects that variant.
pub fn layout(map_focused: bool) -> Panes {
    let window = Rect { x: 0, y: 0, w: 1280, h: 800 };
    let console = Rect { x: 0, y: 640, w: 1280, h: 160 };
    let status = Rect { x: 0, y: 0, w: 240, h: 640 };
    if map_focused {
        Panes {
            window,
            console,
            status,
            actions: Rect { x: 0, y: 0, w: 0, h: 0 },
            notify: Rect { x: 1040, y: 0, w: 240, h: 80 },
            map: Rect { x: 240, y: 80, w: 1040, h: 560 },
        }
    } else {
        Panes {
            window,
            console,
            status,
            actions: Rect { x: 440, y: 0, w: 400, h: 120 },
            notify: Rect { x: 1040, y: 0, w: 240, h: 80 },
            map: Rect { x: 840, y: 80, w: 440, h: 560 },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panes_fit_window_without_overlap() {
        for focused in [false, true] {
            let p = layout(focused);
            for pane in [&p.console, &p.status, &p.notify, &p.map] {
                assert!(p.window.contains(pane), "pane outside window: {pane:?}");
            }
            assert!(!p.status.overlaps(&p.map));
            assert!(!p.console.overlaps(&p.map));
            assert!(!p.notify.overlaps(&p.map));
            if !focused {
                assert!(p.window.contains(&p.actions));
                assert!(!p.actions.overlaps(&p.map));
            }
        }
    }

    #[test]
    fn map_focus_enlarges_map() {
        let a = layout(false);
        let b = layout(true);
        assert!(b.map.w * b.map.h > a.map.w * a.map.h);
    }
}
