//! Device frames: a generic phone or tablet outline built from ordinary nodes (a body, a screen and
//! a camera cut-out), so a capture can be swapped into the screen and the layout stays.
//!
//! These are neutral shapes with typical proportions, not reproductions of any manufacturer's
//! artwork. The result is a plain `create` payload: it goes through the same validation, history
//! and agent attribution as anything else.

use od_core::{ImageFit, NewNode, NodeKind, Paint, Stroke};

/// The name of the screen node inside a device: the one to put a capture in.
pub const SCREEN_NAME: &str = "Screen";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Device {
    /// Tall phone with a pill-shaped camera island.
    Phone,
    /// Tall phone with a round punch-hole camera.
    PhonePunchHole,
    /// 4:3 tablet.
    Tablet,
    /// 16:10 tablet.
    TabletWide,
}

struct Shape {
    /// Screen height divided by screen width.
    aspect: f64,
    /// Bezel thickness as a share of the screen width.
    bezel: f64,
    /// Outer corner radius as a share of the screen width.
    corner: f64,
}

impl Device {
    pub const ALL: [Device; 4] = [
        Device::Phone,
        Device::PhonePunchHole,
        Device::Tablet,
        Device::TabletWide,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Device::Phone => "phone",
            Device::PhonePunchHole => "phone-punch-hole",
            Device::Tablet => "tablet",
            Device::TabletWide => "tablet-wide",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Device::Phone => "Phone (camera island)",
            Device::PhonePunchHole => "Phone (punch-hole camera)",
            Device::Tablet => "Tablet 4:3",
            Device::TabletWide => "Tablet 16:10",
        }
    }

    pub fn parse(id: &str) -> Option<Device> {
        Device::ALL.into_iter().find(|d| d.id() == id)
    }

    fn shape(self) -> Shape {
        match self {
            Device::Phone => Shape {
                aspect: 2796.0 / 1290.0,
                bezel: 0.03,
                corner: 0.16,
            },
            Device::PhonePunchHole => Shape {
                aspect: 2400.0 / 1080.0,
                bezel: 0.025,
                corner: 0.12,
            },
            Device::Tablet => Shape {
                aspect: 2732.0 / 2048.0,
                bezel: 0.04,
                corner: 0.07,
            },
            Device::TabletWide => Shape {
                aspect: 2560.0 / 1600.0,
                bezel: 0.03,
                corner: 0.05,
            },
        }
    }
}

fn color(hex: &str) -> Option<Paint> {
    Paint::parse(hex).ok()
}

fn rect(name: &str, kind: NodeKind, x: f64, y: f64, w: f64, h: f64, radius: f64) -> NewNode {
    let mut n = NewNode::new(kind);
    n.name = Some(name.to_string());
    n.x = round(x);
    n.y = round(y);
    n.width = round(w);
    n.height = round(h);
    if radius > 0.0 {
        n.corner_radius = Some(round(radius));
    }
    n
}

fn round(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// A group named after the device, holding the body, the screen and the camera. `screen_width` is
/// the width of the screen in document units; the group is as wide as the screen plus both
/// bezels, positioned at the origin (move the group to place it). With `screen_image` (a
/// `media/…` reference) the screen shows that image; otherwise it is a dark placeholder.
pub fn template(device: Device, screen_width: f64, screen_image: Option<&str>) -> NewNode {
    let shape = device.shape();
    let (sw, sh) = (screen_width, screen_width * shape.aspect);
    let bezel = sw * shape.bezel;
    let (bw, bh) = (sw + 2.0 * bezel, sh + 2.0 * bezel);
    let outer = sw * shape.corner;
    let inner = (outer - bezel).max(0.0);

    let mut group = NewNode::new(NodeKind::Group);
    group.name = Some(format!("{} frame", device.label()));

    let mut body = rect("Body", NodeKind::Rect, 0.0, 0.0, bw, bh, outer);
    body.fill = color("#17171a");
    body.stroke = color("#3b3b42").map(|paint| Stroke { paint, width: 2.0 });
    group.children.push(body);

    let screen = match screen_image {
        Some(reference) => {
            let mut n = rect(SCREEN_NAME, NodeKind::Image, bezel, bezel, sw, sh, inner);
            n.image = Some(reference.to_string());
            n.image_fit = Some(ImageFit::Cover);
            n
        }
        None => {
            let mut n = rect(SCREEN_NAME, NodeKind::Rect, bezel, bezel, sw, sh, inner);
            n.fill = color("#0b0b0d");
            n
        }
    };
    group.children.push(screen);

    let centre = bw / 2.0;
    let mut camera = match device {
        Device::Phone => {
            let (w, h) = (sw * 0.28, sw * 0.08);
            rect(
                "Camera island",
                NodeKind::Rect,
                centre - w / 2.0,
                bezel + sw * 0.03,
                w,
                h,
                h / 2.0,
            )
        }
        Device::PhonePunchHole => {
            let d = sw * 0.045;
            rect(
                "Camera",
                NodeKind::Ellipse,
                centre - d / 2.0,
                bezel + sw * 0.025,
                d,
                d,
                0.0,
            )
        }
        Device::Tablet | Device::TabletWide => {
            let d = sw * 0.012;
            rect(
                "Camera",
                NodeKind::Ellipse,
                centre - d / 2.0,
                (bezel - d) / 2.0,
                d,
                d,
                0.0,
            )
        }
    };
    camera.fill = color("#000000");
    group.children.push(camera);
    group
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use od_core::{Actor, Document, Operation, Session};

    fn placed(device: Device, width: f64, image: Option<&str>) -> Session {
        let mut s = Session::new(Document::new("t"));
        let page = s.add_page("p").unwrap();
        let root = s.document().page(&page).unwrap().root.id.clone();
        s.apply(
            Actor::User,
            None,
            &[Operation::Create {
                parent: root,
                index: None,
                node: template(device, width, image),
            }],
        )
        .unwrap();
        s
    }

    #[test]
    fn every_device_is_a_valid_group_with_body_screen_and_camera() {
        for device in Device::ALL {
            let s = placed(device, 1290.0, None);
            let page = &s.document().pages[0];
            let group = &page.root.children[0];
            let names: Vec<&str> = group.children.iter().map(|c| c.name.as_str()).collect();
            assert_eq!(names[..2], ["Body", SCREEN_NAME], "{device:?}");
            assert_eq!(group.children.len(), 3, "{device:?} has a camera");
            let (body, screen) = (&group.children[0], &group.children[1]);
            assert!(
                screen.x > 0.0 && screen.x + screen.width < body.width,
                "{device:?}: bezel on both sides"
            );
            assert!((screen.width - 1290.0).abs() < 0.01);
            assert!(
                screen.corner_radius < body.corner_radius,
                "{device:?}: the screen is rounder inside"
            );
        }
    }

    #[test]
    fn a_phone_screen_has_the_proportions_of_a_phone_capture() {
        let s = placed(Device::Phone, 1290.0, None);
        let screen = &s.document().pages[0].root.children[0].children[1];
        assert!((screen.height - 2796.0).abs() < 1.0, "{}", screen.height);
    }

    #[test]
    fn a_capture_goes_into_the_screen_as_a_covering_image() {
        let s = placed(Device::Phone, 600.0, Some("media/home.png"));
        let screen = &s.document().pages[0].root.children[0].children[1];
        assert_eq!(screen.kind, NodeKind::Image);
        assert_eq!(screen.image.as_deref(), Some("media/home.png"));
        assert_eq!(screen.image_fit, Some(ImageFit::Cover));
    }

    #[test]
    fn an_oversized_device_is_refused_by_the_core_not_built_wrong() {
        let mut s = Session::new(Document::new("t"));
        let page = s.add_page("p").unwrap();
        let root = s.document().page(&page).unwrap().root.id.clone();
        let result = s.apply(
            Actor::User,
            None,
            &[Operation::Create {
                parent: root,
                index: None,
                node: template(Device::Phone, 9000.0, None),
            }],
        );
        assert!(
            result.is_err(),
            "9000 wide is 19,500 tall, over the size limit"
        );
    }

    #[test]
    fn ids_round_trip() {
        for d in Device::ALL {
            assert_eq!(Device::parse(d.id()), Some(d));
        }
        assert_eq!(Device::parse("watch"), None);
    }
}
