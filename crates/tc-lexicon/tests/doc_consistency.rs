//! Documentation counts must match the generated lexicon tables.

use std::path::PathBuf;

use tc_lexicon::WORDNET_ENTRIES;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn anchor_count(text: &str) -> usize {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .windows(2)
        .find_map(|pair| {
            (pair[1] == "exact")
                .then(|| pair[0].parse::<usize>().ok())
                .flatten()
        })
        .expect("exact anchor count")
}

#[test]
fn readme_lexicon_counts_match_generated_tables() {
    let root = root();
    let readme = std::fs::read_to_string(root.join("README.md")).expect("README");
    let provenance =
        std::fs::read_to_string(root.join("goldmines/PROVENANCE.md")).expect("PROVENANCE");
    let rows = WORDNET_ENTRIES.len();
    assert!(
        readme.contains(&format!("**{rows}** WordNet rows")),
        "README WordNet row count is stale; expected {rows}"
    );
    assert!(
        provenance.contains(&format!("**{rows}** emitted `WordNetEntry` rows")),
        "PROVENANCE WordNet row count is stale; expected {rows}"
    );
    let readme_anchors = anchor_count(&readme);
    let provenance_anchors = anchor_count(&provenance);
    assert_eq!(
        readme_anchors, provenance_anchors,
        "README and PROVENANCE anchor counts disagree"
    );
    assert_eq!(readme_anchors, 43, "unexpected anchor count");
}

#[test]
fn readme_version_matches_workspace_package_version() {
    let readme = std::fs::read_to_string(root().join("README.md")).expect("README");
    let version = env!("CARGO_PKG_VERSION");
    assert!(
        readme.contains(&format!("`tc_r1` {version}")),
        "README version label is stale; expected `tc_r1` {version}"
    );
    assert!(!readme.contains("`tc_r1` v1.0"), "stale v1.0 label");
    assert!(
        readme.contains("All Rights Reserved"),
        "README license label is stale"
    );
    assert!(
        readme.contains("Build Requirements"),
        "missing build requirements"
    );
    assert!(
        readme.contains("Scope & Verification Note"),
        "missing scope note"
    );
}
