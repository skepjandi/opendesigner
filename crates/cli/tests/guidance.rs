#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::process::{Command, Output};

fn od(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_opendesigner"))
        .args(args)
        .output()
        .unwrap()
}

fn ok(args: &[&str]) -> String {
    let out = od(args);
    assert!(
        out.status.success(),
        "{args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

/// The guidance loop from the terminal: the user pins a note, the agent reads it with the
/// selection, acts through operations, and resolves the pin.
#[test]
fn pin_read_act_resolve() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("p.od");
    let d = dir.to_str().unwrap();
    ok(&["new", d]);
    let page: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.join("pages/p_1.json")).unwrap()).unwrap();
    let root = page["root"]["id"].as_str().unwrap().to_string();

    let ops = tmp.path().join("ops.json");
    fs::write(
        &ops,
        serde_json::json!([{"op":"create","parent":root,
            "node":{"kind":"text","name":"Headline","text":"Hi","font_size":24}}])
        .to_string(),
    )
    .unwrap();
    let receipt: serde_json::Value =
        serde_json::from_str(&ok(&["apply", d, ops.to_str().unwrap()])).unwrap();
    let headline = receipt["created"][0].as_str().unwrap().to_string();

    assert_eq!(
        ok(&["annotate", d, "headline too small", "--node", &headline]).trim(),
        "annotation 1"
    );
    ok(&[
        "annotate",
        d,
        "more air here",
        "--page",
        "p_1",
        "--region",
        "0,0,100,40",
    ]);

    let g: serde_json::Value =
        serde_json::from_str(&ok(&["guidance", d, "--select", &headline])).unwrap();
    assert_eq!(g["selection"][0]["node"]["name"], "Headline");
    assert_eq!(g["annotations"].as_array().unwrap().len(), 2);
    assert_eq!(g["annotations"][0]["note"], "headline too small");
    assert_eq!(g["annotations"][0]["target_exists"], true);

    ok(&[
        "resolve",
        d,
        "1",
        "--note",
        "raised to 96",
        "--agent",
        "claude-code",
    ]);
    let open = ok(&["annotations", d]);
    assert_eq!(open.lines().count(), 1);
    let all = ok(&["annotations", d, "--all"]);
    assert!(all.contains("raised to 96") && all.contains("agent:claude-code"));

    let bad = od(&["annotate", d, "x", "--node", "n_999"]);
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("n_999 does not exist"));
}
