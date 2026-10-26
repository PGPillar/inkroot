//! R42: the window may open only Inkroot's own project URLs.

#[test]
fn opener_is_scoped_to_inkroot_urls() {
    let v: serde_json::Value =
        serde_json::from_str(include_str!("../capabilities/default.json")).unwrap();
    let perms = v["permissions"].as_array().unwrap();
    let plain: Vec<&str> = perms.iter().filter_map(|p| p.as_str()).collect();
    assert_eq!(
        plain,
        [
            "core:default",
            "dialog:allow-open",
            "opener:allow-reveal-item-in-dir",
            "core:window:allow-close",
            "core:window:allow-destroy"
        ]
    );
    let scoped: Vec<&serde_json::Value> = perms.iter().filter(|p| p.is_object()).collect();
    assert_eq!(scoped.len(), 1);
    assert_eq!(scoped[0]["identifier"], "opener:allow-open-url");
    assert_eq!(
        scoped[0]["allow"],
        serde_json::json!([
            { "url": "https://github.com/n0dal/inkroot" },
            { "url": "https://github.com/n0dal/inkroot/*" },
            { "url": "https://f-droid.org/packages/de.szalkowski.activitylauncher/" },
            { "url": "https://play.google.com/store/apps/details?id=de.szalkowski.activitylauncher" },
            { "url": "https://n0dal.github.io/inkroot/*" }
        ])
    );
    assert!(scoped[0].get("deny").is_none());
    let text = include_str!("../capabilities/default.json");
    assert!(!text.contains("allow-default-urls"));
}

/// Every URL the UI can open (app/src/lib/links.ts) must match an allowed pattern.
#[test]
fn every_ui_link_is_allowed() {
    let v: serde_json::Value =
        serde_json::from_str(include_str!("../capabilities/default.json")).unwrap();
    let patterns: Vec<String> = v["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p.is_object())
        .flat_map(|p| p["allow"].as_array().unwrap().clone())
        .map(|a| a["url"].as_str().unwrap().to_string())
        .collect();
    let allowed = |u: &str| {
        patterns.iter().any(|p| match p.strip_suffix('*') {
            Some(prefix) => u.starts_with(prefix),
            None => u == p,
        })
    };
    let links = include_str!("../../src/lib/links.ts");
    let mut repo = String::new();
    let mut urls = Vec::new();
    for stmt in links.split(';') {
        let stmt = stmt.trim();
        if !stmt.starts_with("export const") {
            continue;
        }
        let lits: Vec<&str> = stmt.split('\'').skip(1).step_by(2).collect();
        if stmt.contains("REPO +") {
            urls.push(format!("{repo}{}", lits[0]));
        } else if let Some(l) = lits.first() {
            if stmt.contains("const REPO ") {
                repo = l.to_string();
            }
            urls.push(l.to_string());
        }
    }
    // The issue-link function builds REPO + '/issues/new?...'.
    urls.push(format!("{repo}/issues/new?title=x"));
    assert!(
        urls.len() >= 6,
        "links.ts parse found too few URLs: {urls:?}"
    );
    for u in &urls {
        assert!(allowed(u), "UI link not allowed by opener scope: {u}");
    }
}
