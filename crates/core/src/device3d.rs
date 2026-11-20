//! A device in 3D: a phone, tablet, laptop or monitor drawn by `render` from a generated model,
//! turned and tilted in space, with a capture or a recording on its screen
//! (`docs/wiki/document-format.md#3d-devices`).
//!
//! The node's box is the viewport the device is drawn in; the device itself is data (which model,
//! how it is turned, how it is lit), so an agent edits it with the same `set` operations as any
//! other node and a video animates it with keyframes (`crate::animation`).

use serde::{Deserialize, Serialize};

use crate::error::OpError;
use crate::limits::finite_in;
use crate::video::{Footage, MAX_IN_MS};

/// Largest turn in degrees either way: ten whole turns, enough for any spin a video keyframes.
pub const MAX_ROTATION: f64 = 3_600.0;
pub const MIN_ZOOM: f64 = 0.05;
pub const MAX_ZOOM: f64 = 10.0;
/// Field of view in degrees: small is nearly flat (a long lens), large is a wide-angle look.
pub const MIN_FOV: f64 = 1.0;
pub const MAX_FOV: f64 = 100.0;
pub const MAX_LIGHT_ANGLE: f64 = 360.0;

/// Which generated model is drawn. None is a manufacturer's artwork: proportions are typical of
/// the kind, so a capture fits, and the outline is neutral (`docs/wiki/roadmap.md`, decision log).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DeviceModel {
    /// A modern phone: thin even bezel, a pill cut-out at the top, a camera island on the back.
    #[default]
    Phone,
    /// A phone with a round punch-hole camera, as many Android phones have.
    PhonePunchHole,
    /// A 4:3 tablet with an even bezel.
    Tablet,
    /// A laptop opened past upright: the lid shows the screen, the base a keyboard.
    Laptop,
    /// A desktop monitor on a stand.
    Monitor,
}

impl DeviceModel {
    pub const ALL: [DeviceModel; 5] = [
        DeviceModel::Phone,
        DeviceModel::PhonePunchHole,
        DeviceModel::Tablet,
        DeviceModel::Laptop,
        DeviceModel::Monitor,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            DeviceModel::Phone => "phone",
            DeviceModel::PhonePunchHole => "phone_punch_hole",
            DeviceModel::Tablet => "tablet",
            DeviceModel::Laptop => "laptop",
            DeviceModel::Monitor => "monitor",
        }
    }

    /// Width over height of the screen in portrait (phones, tablet) or landscape (laptop,
    /// monitor), so a capture of the right shape fills it without cropping.
    pub fn screen_aspect(self) -> f64 {
        match self {
            DeviceModel::Phone | DeviceModel::PhonePunchHole => 1206.0 / 2622.0,
            DeviceModel::Tablet => 3.0 / 4.0,
            DeviceModel::Laptop => 16.0 / 10.0,
            DeviceModel::Monitor => 16.0 / 9.0,
        }
    }
}

/// How a 3D device looks. Angles are degrees; the device faces the viewer at all zeros.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Device3d {
    #[serde(default)]
    pub model: DeviceModel,
    /// Tilt: positive leans the top away from the viewer.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rotate_x: f64,
    /// Turn: positive turns the device's right edge away, showing its left side.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rotate_y: f64,
    /// Roll in the picture plane: positive turns clockwise.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rotate_z: f64,
    /// Size of the device in its box: 1 fits it with a margin whatever its turn.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub zoom: f64,
    /// Lens: degrees of view. 30 is a natural product shot; 1 is nearly flat.
    #[serde(default = "default_fov", skip_serializing_if = "is_default_fov")]
    pub fov: f64,
    /// Where the key light comes from, in degrees around the device (0 above, 90 from the right).
    #[serde(default = "default_light", skip_serializing_if = "is_default_light")]
    pub light_angle: f64,
    /// Strength of the soft shadow under the device, 0 to 1.
    #[serde(default = "default_shadow", skip_serializing_if = "is_default_shadow")]
    pub shadow: f64,
    /// Strength of the reflection on the screen glass, 0 to 1.
    #[serde(default = "default_glare", skip_serializing_if = "is_default_glare")]
    pub glare: f64,
    /// Where a capture taller than the screen is shown: 0 its top, 1 its bottom. Unset centres
    /// a cover-fitted capture as an image node does. Animate it to scroll a long page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen_scroll: Option<f64>,
    /// A recording on the screen instead of the node's `image`: in a video it plays from
    /// `in_ms` while the frame is shown; a still shows its picture at `in_ms`. Saved and set as
    /// `screen_footage`.
    #[serde(
        default,
        rename = "screen_footage",
        skip_serializing_if = "Option::is_none"
    )]
    pub footage: Option<Footage>,
}

pub const DEFAULT_FOV: f64 = 30.0;
pub const DEFAULT_LIGHT_ANGLE: f64 = -35.0;
pub const DEFAULT_SHADOW: f64 = 0.35;
pub const DEFAULT_GLARE: f64 = 0.2;

fn one() -> f64 {
    1.0
}
fn is_one(v: &f64) -> bool {
    *v == 1.0
}
fn is_zero(v: &f64) -> bool {
    *v == 0.0
}
fn default_fov() -> f64 {
    DEFAULT_FOV
}
fn is_default_fov(v: &f64) -> bool {
    *v == DEFAULT_FOV
}
fn default_light() -> f64 {
    DEFAULT_LIGHT_ANGLE
}
fn is_default_light(v: &f64) -> bool {
    *v == DEFAULT_LIGHT_ANGLE
}
fn default_shadow() -> f64 {
    DEFAULT_SHADOW
}
fn is_default_shadow(v: &f64) -> bool {
    *v == DEFAULT_SHADOW
}
fn default_glare() -> f64 {
    DEFAULT_GLARE
}
fn is_default_glare(v: &f64) -> bool {
    *v == DEFAULT_GLARE
}

impl Default for Device3d {
    fn default() -> Device3d {
        Device3d {
            model: DeviceModel::default(),
            rotate_x: 0.0,
            rotate_y: 0.0,
            rotate_z: 0.0,
            zoom: 1.0,
            fov: DEFAULT_FOV,
            light_angle: DEFAULT_LIGHT_ANGLE,
            shadow: DEFAULT_SHADOW,
            glare: DEFAULT_GLARE,
            screen_scroll: None,
            footage: None,
        }
    }
}

pub fn rotation(what: &'static str, v: f64) -> Result<f64, OpError> {
    finite_in(what, v, -MAX_ROTATION, MAX_ROTATION)
}
pub fn zoom(v: f64) -> Result<f64, OpError> {
    finite_in("zoom", v, MIN_ZOOM, MAX_ZOOM)
}
pub fn fov(v: f64) -> Result<f64, OpError> {
    finite_in("fov", v, MIN_FOV, MAX_FOV)
}
pub fn light_angle(v: f64) -> Result<f64, OpError> {
    finite_in("light_angle", v, -MAX_LIGHT_ANGLE, MAX_LIGHT_ANGLE)
}
pub fn unit(what: &'static str, v: f64) -> Result<f64, OpError> {
    finite_in(what, v, 0.0, 1.0)
}

/// A recording on a screen names a video file in `media/` and starts inside a day of it.
pub fn check_footage(f: &Footage) -> Result<(), OpError> {
    crate::media::footage_name(&f.file)?;
    finite_in("footage in_ms", f.in_ms as f64, 0.0, MAX_IN_MS as f64).map(|_| ())
}

impl Device3d {
    /// Every field within its bounds (loading and `Restore`).
    pub fn check(&self) -> Result<(), OpError> {
        rotation("rotate_x", self.rotate_x)?;
        rotation("rotate_y", self.rotate_y)?;
        rotation("rotate_z", self.rotate_z)?;
        zoom(self.zoom)?;
        fov(self.fov)?;
        light_angle(self.light_angle)?;
        unit("shadow", self.shadow)?;
        unit("glare", self.glare)?;
        if let Some(s) = self.screen_scroll {
            unit("screen_scroll", s)?;
        }
        if let Some(f) = &self.footage {
            check_footage(f)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_omitted_so_a_plain_device_saves_as_its_model_only() {
        let json = serde_json::to_string(&Device3d::default()).unwrap();
        assert_eq!(json, r#"{"model":"phone"}"#);
        let back: Device3d = serde_json::from_str("{}").unwrap();
        assert_eq!(back, Device3d::default());
    }

    #[test]
    fn bounds_are_enforced_on_every_field() {
        let mut d = Device3d {
            rotate_y: 3_601.0,
            ..Device3d::default()
        };
        assert!(d.check().is_err());
        d.rotate_y = -720.0;
        d.screen_scroll = Some(1.5);
        assert!(d.check().is_err());
        d.screen_scroll = Some(0.25);
        d.footage = Some(Footage {
            file: "media/../x.mp4".into(),
            in_ms: 0,
        });
        assert!(d.check().is_err());
        d.footage = Some(Footage {
            file: "media/sim.mov".into(),
            in_ms: 500,
        });
        assert!(d.check().is_ok());
    }
}
