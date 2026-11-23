#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! `image_smoothing`: a per-image choice between filtered and nearest-neighbour scaling, and the
//! format version that introduced it.

use od_core::{
    Actor, Document, ImageSmoothing, NewNode, NodeId, NodeKind, Operation, Property, Session,
    project,
};

fn session_with_image() -> (Session, NodeId) {
    let mut s = Session::new(Document::new("smoothing"));
    let page = s.add_page("p").unwrap();
    let root = s.document().page(&page).unwrap().root.id.clone();
    let mut node = NewNode::new(NodeKind::Image);
    node.width = 48.0;
    node.height = 48.0;
    node.image = Some("media/hero.png".into());
    let id = s
        .apply(
            Actor::User,
            None,
            &[Operation::Create {
                parent: root,
                index: None,
                node,
            }],
        )
        .unwrap()
        .created[0]
        .clone();
    (s, id)
}

fn set(s: &mut Session, id: &NodeId, value: Option<ImageSmoothing>) {
    s.apply(
        Actor::User,
        None,
        &[Operation::Set {
            id: id.clone(),
            prop: Property::ImageSmoothing(value),
        }],
    )
    .unwrap();
}

#[test]
fn smoothing_is_left_out_of_the_file_until_it_is_nearest() {
    let (mut s, id) = session_with_image();
    let json = |s: &Session| serde_json::to_value(s.document().node(&id).unwrap()).unwrap();
    assert!(json(&s).get("image_smoothing").is_none());
    set(&mut s, &id, Some(ImageSmoothing::Nearest));
    assert_eq!(json(&s)["image_smoothing"], "nearest");
    set(&mut s, &id, Some(ImageSmoothing::Smooth));
    assert!(
        json(&s).get("image_smoothing").is_none(),
        "smooth is the default and is not written"
    );
}

#[test]
fn smoothing_undoes_and_survives_a_save() {
    let (mut s, id) = session_with_image();
    set(&mut s, &id, Some(ImageSmoothing::Nearest));
    let dir = tempfile::tempdir().unwrap();
    project::save(dir.path(), &s).unwrap();
    let loaded = project::load(dir.path()).unwrap();
    assert_eq!(
        loaded.document().node(&id).unwrap().image_smoothing,
        Some(ImageSmoothing::Nearest)
    );
    s.undo(Actor::User).unwrap();
    assert_eq!(s.document().node(&id).unwrap().image_smoothing, None);
}

#[test]
fn smoothing_applies_to_images_only() {
    let mut s = Session::new(Document::new("smoothing"));
    let page = s.add_page("p").unwrap();
    let root = s.document().page(&page).unwrap().root.id.clone();
    let rect = s
        .apply(
            Actor::User,
            None,
            &[Operation::Create {
                parent: root,
                index: None,
                node: NewNode::new(NodeKind::Rect),
            }],
        )
        .unwrap()
        .created[0]
        .clone();
    let err = s
        .apply(
            Actor::User,
            None,
            &[Operation::Set {
                id: rect,
                prop: Property::ImageSmoothing(Some(ImageSmoothing::Nearest)),
            }],
        )
        .unwrap_err();
    assert!(err.to_string().contains("image_smoothing"), "{err}");
}

#[test]
fn a_version_six_project_still_loads_and_is_saved_as_version_seven() {
    let (s, id) = session_with_image();
    let dir = tempfile::tempdir().unwrap();
    project::save(dir.path(), &s).unwrap();
    let manifest = dir.path().join("od.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    assert!(text.contains("format = 7"));
    std::fs::write(&manifest, text.replace("format = 7", "format = 6")).unwrap();
    let loaded = project::load(dir.path()).unwrap();
    assert!(loaded.document().node(&id).is_some());
    assert_eq!(loaded.document().format, 7);
    let again = tempfile::tempdir().unwrap();
    project::save(again.path(), &loaded).unwrap();
    assert!(
        std::fs::read_to_string(again.path().join("od.toml"))
            .unwrap()
            .contains("format = 7")
    );
}

#[test]
fn a_newer_project_is_refused() {
    let (s, _) = session_with_image();
    let dir = tempfile::tempdir().unwrap();
    project::save(dir.path(), &s).unwrap();
    let manifest = dir.path().join("od.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(&manifest, text.replace("format = 7", "format = 8")).unwrap();
    assert!(project::load(dir.path()).is_err());
}
