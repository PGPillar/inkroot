use inkroot_lib::demo::{DemoScenario, demo_deps, demo_profile};
use inkroot_lib::events::Emitter;
use inkroot_lib::fakes::fake_boot_image;
use inkroot_lib::flows::FlowEngine;
use inkroot_lib::ipc::{
    DeviceStatus, FlowKind, FlowOptions, FlowState, FlowStatus, LogLine, Progress, SettingsPatch,
};
use inkroot_lib::settings::RISK_NOTICE_VERSION;
use std::sync::Arc;
use std::time::{Duration, Instant};

struct Quiet;
impl Emitter for Quiet {
    fn device(&self, _: &DeviceStatus) {}
    fn flow_state(&self, _: &FlowState) {}
    fn progress(&self, _: &Progress) {}
    fn log(&self, _: &LogLine) {}
}

/// Answers prompts (matched in order by contract literal) until the flow ends.
fn drive(engine: &FlowEngine, id: &str, answers: &[(&str, &str)]) -> FlowState {
    let start = Instant::now();
    let mut answered: Vec<String> = vec![];
    loop {
        assert!(start.elapsed() < Duration::from_secs(8), "flow hung");
        let Some(s) = engine.current().filter(|s| s.flow_id == id) else {
            std::thread::sleep(Duration::from_millis(2));
            continue;
        };
        match s.status {
            FlowStatus::Done | FlowStatus::Failed | FlowStatus::Cancelled => return s,
            FlowStatus::WaitingForUser => {
                let p = s.prompt.clone().unwrap();
                if !answered.contains(&p.id) {
                    let lit = serde_json::to_value(p.kind).unwrap();
                    let lit = lit.as_str().unwrap().to_string();
                    let (_, opt) = answers
                        .iter()
                        .find(|(k, _)| *k == lit)
                        .unwrap_or_else(|| panic!("unexpected prompt {lit}: {s:?}"));
                    answered.push(p.id.clone());
                    engine.answer(id, &p.id, opt);
                }
            }
            _ => {}
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn run(scenario: DemoScenario) {
    let deps = demo_deps(scenario, Arc::new(Quiet), 0.0);
    deps.settings
        .apply(SettingsPatch {
            accepted_risk_version: Some(Some(RISK_NOTICE_VERSION)),
            ..SettingsPatch::default()
        })
        .unwrap();
    let library_id = deps.library.list(None).first().map(|s| s.id.clone());
    let engine = FlowEngine::new(deps);
    let imported = tempfile::tempdir().unwrap();
    let (kind, opts, answers): (FlowKind, FlowOptions, Vec<(&str, &str)>) = match scenario {
        DemoScenario::Fresh => (
            FlowKind::Root,
            FlowOptions::default(),
            vec![("cableCheck", "have")],
        ),
        DemoScenario::Rooted => (FlowKind::Unroot, FlowOptions::default(), vec![]),
        DemoScenario::Bricked => (
            FlowKind::Rescue,
            FlowOptions {
                backup_id: Some(library_id.expect("bricked scenario seeds a backup")),
                ..FlowOptions::default()
            },
            vec![("cableCheck", "have")],
        ),
        DemoScenario::RootedNoBackup => {
            let p = imported.path().join("boot_b.img");
            std::fs::write(&p, fake_boot_image(demo_profile().boot_size as usize, 2)).unwrap();
            (
                FlowKind::ImportBackup,
                FlowOptions {
                    image_path: Some(p.to_string_lossy().into_owned()),
                    ..FlowOptions::default()
                },
                vec![],
            )
        }
    };
    let id = engine.start(kind, opts).expect("start");
    let end = drive(&engine, &id, &answers);
    assert_eq!(end.status, FlowStatus::Done, "{scenario:?}: {end:?}");
}

#[test]
fn demo_all_flows_complete_at_speed_zero() {
    let t = Instant::now();
    for s in [
        DemoScenario::Fresh,
        DemoScenario::Rooted,
        DemoScenario::Bricked,
        DemoScenario::RootedNoBackup,
    ] {
        run(s);
    }
    assert!(t.elapsed() < Duration::from_secs(5), "{:?}", t.elapsed());
}

#[test]
fn demo_seeds_backups_only_where_expected() {
    let lib = |s| demo_deps(s, Arc::new(Quiet), 0.0).library.list(None).len();
    assert_eq!(lib(DemoScenario::Fresh), 0);
    assert_eq!(lib(DemoScenario::RootedNoBackup), 0);
    assert_eq!(lib(DemoScenario::Rooted), 1);
    assert_eq!(lib(DemoScenario::Bricked), 1);
}

#[test]
fn demo_risk_notice_not_preaccepted() {
    let d = demo_deps(DemoScenario::Fresh, Arc::new(Quiet), 0.0);
    assert!(!inkroot_lib::settings::risk_accepted(&d.settings.get()));
}

#[test]
fn bricked_tablet_enters_edl_after_a_delay() {
    let d = demo_deps(DemoScenario::Bricked, Arc::new(Quiet), 0.02);
    assert!(d.usb.edl_device().is_none(), "EDL must not be instant");
    std::thread::sleep(Duration::from_millis(120));
    assert!(d.usb.edl_device().is_some());
}
