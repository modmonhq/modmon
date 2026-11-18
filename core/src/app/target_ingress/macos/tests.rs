use super::*;

fn host() -> MacosIngressHost {
    MacosIngressHost {
        haproxy_executable: "/opt/homebrew/bin/haproxy".to_owned(),
        docker_executable: "/Applications/Docker.app/Contents/Resources/bin/docker".to_owned(),
        docker_host: "unix:///srv/Dev & Ops/.docker/run/docker.sock".to_owned(),
        home: "/srv/Dev & Ops".to_owned(),
        network_subnet: "10.252.7.0/24".to_owned(),
        network_gateway: "10.252.7.1".to_owned(),
    }
}

#[test]
fn plist_escapes_xml_and_keeps_arguments_unsplit() {
    let job = LaunchdJob {
        label: "local.mm.test".to_owned(),
        program_arguments: vec!["/opt/a b/<bin>".to_owned(), "it's \"quoted\"".to_owned()],
        environment: vec![("HOME".to_owned(), "/srv/a&b".to_owned())],
        keep_alive: LaunchdKeepAlive::Always,
        throttle_seconds: 3,
        log_path: "/srv/a&b/Library/Logs/mm/test.log".to_owned(),
        umask: Some(0o077),
    };
    let plist = render_launchd_plist(&job);
    assert!(plist.contains("<string>/opt/a b/&lt;bin&gt;</string>"));
    assert!(plist.contains("<string>it&apos;s &quot;quoted&quot;</string>"));
    assert!(plist.contains("<string>/srv/a&amp;b</string>"));
    assert!(plist.contains("<key>KeepAlive</key>\n  <true/>"));
    assert!(plist.contains("<integer>3</integer>"));
    assert!(!plist.contains("a&b"));
    assert!(plist.contains("<key>Umask</key>\n  <integer>63</integer>"));
}

#[test]
fn support_files_pin_executables_endpoint_and_user_paths() {
    let files = render_support_files(&host(), "mm-config", "mm-ingress-x", 24_000, 24_001);
    let names = files
        .iter()
        .map(|file| file.manifest.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            "dynamic.yaml",
            MACOS_HAPROXY_FILE,
            MACOS_FORWARDER_SUPPORT_FILE,
            MACOS_AUTOSTART_SUPPORT_FILE
        ]
    );
    let by_name = |name: &str| {
        files
            .iter()
            .find(|file| file.manifest.name == name)
            .map(|file| String::from_utf8_lossy(&file.bytes).into_owned())
            .unwrap_or_default()
    };
    let forwarder = by_name(MACOS_FORWARDER_SUPPORT_FILE);
    assert!(forwarder.contains("<string>/opt/homebrew/bin/haproxy</string>"));
    assert!(forwarder.contains(
        "<string>/srv/Dev &amp; Ops/Library/Application Support/mm/ingress/haproxy.cfg</string>"
    ));
    assert!(forwarder.contains("<key>KeepAlive</key>\n  <true/>"));
    assert!(!forwarder.contains("DOCKER_HOST"));

    let autostart = by_name(MACOS_AUTOSTART_SUPPORT_FILE);
    assert!(autostart.contains("<string>mm-ingress-x</string>"));
    assert!(
        autostart.contains("<string>unix:///srv/Dev &amp; Ops/.docker/run/docker.sock</string>")
    );
    assert!(autostart.contains(
        "<string>/Applications/Docker.app/Contents/Resources/bin:/usr/bin:/bin:/usr/sbin:/sbin</string>"
    ));
    assert!(autostart.contains("<key>SuccessfulExit</key>\n    <false/>"));
    assert!(by_name(MACOS_HAPROXY_FILE).contains("127.0.0.1:24001 check send-proxy-v2"));

    let destinations = files
        .iter()
        .map(|file| file.manifest.destination.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        destinations[2],
        "/srv/Dev & Ops/Library/LaunchAgents/local.mm.ingress-forwarder.plist"
    );
    assert_eq!(
        destinations[3],
        "/srv/Dev & Ops/Library/LaunchAgents/local.mm.ingress.plist"
    );
}

#[test]
fn dashboard_agent_listens_on_loopback_with_the_pinned_endpoint() {
    let plist = render_dashboard_plist(
        &host(),
        "/srv/Dev & Ops/.local/bin/mm",
        "/srv/Dev & Ops/.mm",
        "docker-compose",
    );
    assert!(plist.contains("<string>local.mm.dashboard</string>"));
    assert!(plist.contains("<string>127.0.0.1:42831</string>"));
    assert!(!plist.contains("0.0.0.0"));
    assert!(plist.contains("<string>/srv/Dev &amp; Ops/.local/bin/mm</string>"));
    assert!(plist.contains("<key>DOCKER_HOST</key>"));
    assert!(plist.contains("<key>SuccessfulExit</key>"));
}
