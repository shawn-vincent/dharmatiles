//! A print list in the editor: its plates, its settings, its refusals.

use super::*;

#[test]
fn a_print_list_opens_on_its_plates_and_its_settings_rewrite_its_file() {
    let dir = std::env::temp_dir().join(format!("dt-plates-view-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("kit.dmap");
    std::fs::write(
        &file,
        "dharmatiles version=1\nmap {\n    // forty flat squares\n    piece \"lib:tests/ground/1x1-flat\" count=40\n}\nplates printer=\"sovol/sv06\" mode=\"single\"\n",
    )
    .unwrap();
    let path = file.to_string_lossy().into_owned();
    let r = start(&[&path], true);
    let t = wait_for(&r, "the plates, built in full", |t| {
        t["status"] == "ready" && t["plan"]["plates"]["exact"] == true
    });
    // the SV06's 220 mm bed holds 5 x 5 squares: 25 and 15
    let p = &t["plan"]["plates"];
    assert_eq!(t["plan"]["kind"], "plates");
    assert_eq!(t["plan"]["sizeMm"], serde_json::json!([220.0, 220.0]));
    assert_eq!(p["layout"][0]["pieces"].as_array().unwrap().len(), 25);
    assert_eq!(p["layout"][1]["pieces"].as_array().unwrap().len(), 15);
    assert_eq!(p["mode"], "single");
    assert_eq!(p["slotCheck"]["ok"], true);
    assert_eq!(p["format"], "stl", "the SV06 is not a Bambu machine");
    assert!(
        p["printers"].as_array().unwrap().len() > 25,
        "the chooser's profiles"
    );
    // the mesh says where its bed is, for the view to draw
    let (head, body) = get(&r, &format!("/api/tiles/{}/mesh", id0(&r)));
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    let n = u32::from_le_bytes(body[4..8].try_into().unwrap()) as usize;
    let meta: Value = serde_json::from_slice(&body[8..8 + n]).unwrap();
    assert_eq!(meta["bed"], serde_json::json!([220.0, 220.0]));
    assert_eq!(meta["plates"], 2);
    assert_eq!(meta["plate"], 0);
    // a setting rewrites the plates line and nothing else; the token is needed
    let token: Value = serde_json::from_slice(&get(&r, "/api/session").1).unwrap();
    let token = token["token"].as_str().unwrap().to_owned();
    let served: Value =
        serde_json::from_slice(&get(&r, &format!("/api/tiles/{}/text", id0(&r))).1).unwrap();
    let body = serde_json::json!({ "list": id0(&r), "key": "gap", "value": "10mm", "docRev": served["docRev"] })
        .to_string();
    assert!(
        post(&r, "/api/plates/set", None, None, &body)
            .0
            .starts_with("HTTP/1.1 403")
    );
    let (head, v) = post(&r, "/api/plates/set", Some(&token), None, &body);
    assert!(head.starts_with("HTTP/1.1 200"), "{head} {v}");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        text.contains("// forty flat squares"),
        "the rest of the file is kept: {text}"
    );
    assert!(
        text.contains("gap=\"10mm\"") && text.contains("printer=\"sovol/sv06\""),
        "{text}"
    );
    // 5 x 35 + 4 x 10 does not fit 210: four to a row, 16 a plate, three plates
    let t = wait_for(&r, "the plates with the wider gap", |t| {
        t["status"] == "ready"
            && t["plan"]["plates"]["gap"] == 10.0
            && t["plan"]["plates"]["layout"]
                .as_array()
                .is_some_and(|l| l.len() == 3)
    });
    assert_eq!(
        t["plan"]["plates"]["layout"][0]["pieces"]
            .as_array()
            .unwrap()
            .len(),
        16
    );
    // a word the plates line has not, or a value it cannot read, is refused and not written
    for (key, value, why) in [
        ("bed", "1x1mm", "not a setting"),
        ("mode", "rainbow", "not a mode"),
    ] {
        let bad = serde_json::json!({ "list": id0(&r), "key": key, "value": value }).to_string();
        let (head, _) = post(&r, "/api/plates/set", Some(&token), None, &bad);
        assert!(head.starts_with("HTTP/1.1 400"), "{why}: {head}");
    }
    assert!(!std::fs::read_to_string(&file).unwrap().contains("rainbow"));
    // the plate in view
    let show = serde_json::json!({ "list": id0(&r), "plate": 1 }).to_string();
    assert!(
        post(&r, "/api/plates/show", Some(&token), None, &show)
            .0
            .starts_with("HTTP/1.1 200")
    );
    let t = wait_for(&r, "plate 2 in view", |t| {
        t["plan"]["plates"]["view"] == 1 && t["status"] == "ready"
    });
    assert_eq!(t["plan"]["plates"]["view"], 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_print_list_that_does_not_plan_still_offers_its_printers() {
    let dir = std::env::temp_dir().join(format!("dt-plates-bad-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("kit.dmap");
    std::fs::write(
        &file,
        "dharmatiles version=1\nmap {\n piece \"lib:tests/plants/1x1-soil+grass\"\n}\nplates printer=\"sovol/sv06\" mode=\"multicolour\"\n",
    )
    .unwrap();
    let path = file.to_string_lossy().into_owned();
    let r = start(&[&path], true);
    let t = wait_for(&r, "the refusal", |t| t["status"] == "failed");
    assert_eq!(t["errors"][0]["code"], "dt::slots");
    assert!(
        t["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("this printer has 1")
    );
    // the panel can still mend it: the printer and the mode are offered
    assert_eq!(t["plan"]["plates"]["planned"], false);
    assert_eq!(t["plan"]["plates"]["printer"], "sovol/sv06");
    assert!(t["plan"]["plates"]["printers"].as_array().unwrap().len() > 25);
    let _ = std::fs::remove_dir_all(dir);
}
