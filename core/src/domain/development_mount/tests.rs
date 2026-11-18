use super::{DevelopmentMountPreset, HostPlatform, is_absolute_container_path};
use std::path::PathBuf;

#[test]
fn presets_platforms_and_container_paths_cover_portable_boundaries() {
    assert_eq!(DevelopmentMountPreset::Laravel.label(), "laravel");
    assert_eq!(DevelopmentMountPreset::Node.label(), "node");
    assert_eq!(DevelopmentMountPreset::Dotnet.label(), "dotnet");
    assert_eq!(DevelopmentMountPreset::Rust.label(), "rust");
    assert_eq!(HostPlatform::current(), HostPlatform::Unix);

    for platform in [HostPlatform::Unix, HostPlatform::Windows] {
        assert_eq!(
            platform.normalize_relative("modules\\api/./src"),
            Ok(PathBuf::from("modules/api/src"))
        );
        assert_eq!(platform.normalize_relative("."), Ok(PathBuf::from(".")));
        for value in [
            "",
            "/absolute",
            "//server/share",
            "C:/drive",
            "../outside",
            "bad\nname",
        ] {
            assert!(
                platform.normalize_relative(value).is_err(),
                "path: {value:?}"
            );
        }
    }

    assert!(is_absolute_container_path("/workspace/api"));
    for value in [
        "",
        "/",
        "relative",
        "/double//slash",
        "/parent/../escape",
        "/bad\npath",
    ] {
        assert!(
            !is_absolute_container_path(value),
            "container path: {value:?}"
        );
    }
}
