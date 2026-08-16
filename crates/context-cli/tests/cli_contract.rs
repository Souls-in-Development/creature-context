use std::{fs, path::PathBuf, process::Command};

fn temp_root() -> PathBuf {
    let path = std::env::temp_dir().join(format!("creature-context-cli-{}", std::process::id()));
    if path.exists() {
        fs::remove_dir_all(&path).unwrap();
    }
    fs::create_dir_all(path.join("src")).unwrap();
    fs::write(
        path.join("PURPOSE.md"),
        "# CLI Fixture\n\n## Goals\n- Exercise the CLI\n",
    )
    .unwrap();
    fs::write(path.join("src/main.rs"), "fn main() {}\n").unwrap();
    path
}

#[test]
fn init_scan_status_and_galaxy_orbit_work() {
    let binary = env!("CARGO_BIN_EXE_creature-context");
    let root = temp_root();
    let galaxy = root.file_name().unwrap().to_str().unwrap();
    for args in [
        ["init", root.to_str().unwrap(), "--format", "json"],
        ["scan", root.to_str().unwrap(), "--format", "json"],
        ["status", root.to_str().unwrap(), "--format", "json"],
    ] {
        let output = Command::new(binary).args(args).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains('{'));
    }
    let first_atlas = fs::read(root.join("ATLAS.idx")).unwrap();
    let output = Command::new(binary)
        .current_dir(&root)
        .args(["scan", ".", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read(root.join("ATLAS.idx")).unwrap(), first_atlas);
    for (axis, proof, producer) in [
        ("integration", "build", "cli-contract-build"),
        ("verification", "test", "cli-contract-test"),
        ("freshness", "metadata", "cli-contract-freshness"),
        ("coherence", "metadata", "cli-contract-coherence"),
    ] {
        let output = Command::new(binary)
            .args([
                "evidence",
                root.to_str().unwrap(),
                galaxy,
                "--axis",
                axis,
                "--proof",
                proof,
                "--producer",
                producer,
                "--recursive",
                "--format",
                "json",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let output = Command::new(binary)
        .args(["green", root.to_str().unwrap(), galaxy, "--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .to_lowercase()
            .contains("\"green\""),
        "Output was: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let output = Command::new(binary)
        .args([
            "orbit",
            root.to_str().unwrap(),
            "--task",
            "understand CLI Fixture",
            "--scale",
            "galaxy",
            "--budget",
            "10000",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("architectural_spine"));
    assert!(
        root.join("ATLAS.idx").exists(),
        "ATLAS.idx should be generated"
    );
    // We don't check for .atlas.yaml or .module-map.yaml here by default
    fs::remove_dir_all(root).unwrap();
}

/// A scan must leave a Green baseline that reflects the *enriched* structure,
/// so `green` and `status` are meaningful immediately without an `evidence`
/// command. `scan_project` evaluates before enrichment adds symbols and sockets;
/// unless the CLI re-evaluates afterwards, the stored assessment is stale — a
/// symbol carries no assessment at all, and a file's integration axis cannot see
/// the sockets enrichment just gave it. This is the fix: evaluate after enrich
/// and reconcile.
#[test]
fn scan_leaves_a_green_baseline_over_the_enriched_structure() {
    let binary = env!("CARGO_BIN_EXE_creature-context");
    let root = std::env::temp_dir().join(format!(
        "creature-context-cli-baseline-{}",
        std::process::id()
    ));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("PURPOSE.md"),
        "# Baseline\n\n## Goals\n- baseline\n",
    )
    .unwrap();
    // A symbol plus an unmet intra-repo import (`nowhere`/`Absent` appear nowhere
    // else, so the socket genuinely holes).
    fs::write(
        root.join("src/main.rs"),
        "use crate::nowhere::Absent;\npub fn build() -> Absent { todo!() }\n",
    )
    .unwrap();

    // Init and scan only — deliberately no `evidence` command.
    for args in [
        ["init", root.to_str().unwrap(), "--format", "json"],
        ["scan", root.to_str().unwrap(), "--format", "json"],
    ] {
        let out = Command::new(binary).args(args).output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    let green = |reference: &str| {
        let out = Command::new(binary)
            .args([
                "green",
                root.to_str().unwrap(),
                reference,
                "--explain",
                "--format",
                "json",
            ])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    };

    // The parsed symbol carries an evaluated assessment — not the null of an
    // entity added after evaluation and never scored.
    let build = green("build");
    assert!(
        build.contains("\"axes\""),
        "a symbol must be evaluated at scan time, got: {build}"
    );

    // The file's integration axis reflects the socket enrichment added: the
    // unmet import is named in a coherence/integration reason, which is only
    // possible if green was recomputed after enrichment.
    let file = green("src/main.rs");
    assert!(
        file.contains("crate::nowhere::Absent"),
        "the unmet socket must darken integration at scan time, got: {file}"
    );

    fs::remove_dir_all(&root).unwrap();
}
