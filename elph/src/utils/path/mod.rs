mod app_paths;
mod resolver;

pub use app_paths::AppPaths;
pub use resolver::{PathResolver, ResolvedPaths};

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    const TEST_RESOLVER: PathResolver = PathResolver {
        home_env: "TEST_AGENT_HOME",
        data_env: "TEST_AGENT_DATA",
        project_env: "TEST_AGENT_PROJECT",
        config_dir_name: ".test-agent",
        data_dir_name: "test-agent",
    };

    struct TestPaths {
        config_dir: PathBuf,
        data_dir: PathBuf,
    }

    impl AppPaths for TestPaths {
        fn config_dir(&self) -> &PathBuf {
            &self.config_dir
        }

        fn data_dir(&self) -> &PathBuf {
            &self.data_dir
        }
    }

    #[test]
    fn resolves_from_explicit_dirs() {
        let paths = ResolvedPaths::from_dirs(PathBuf::from("/cfg"), PathBuf::from("/data"), PathBuf::from("/repo"));

        assert_eq!(paths.config_dir, PathBuf::from("/cfg"));
        assert_eq!(paths.data_dir, PathBuf::from("/data"));
        assert_eq!(paths.project_dir, PathBuf::from("/repo"));
    }

    #[test]
    fn resolver_exposes_static_names() {
        assert_eq!(TEST_RESOLVER.config_dir_name, ".test-agent");
        assert_eq!(TEST_RESOLVER.data_dir_name, "test-agent");
    }

    #[test]
    fn app_paths_builds_expected_file_paths() {
        let paths = TestPaths {
            config_dir: PathBuf::from("/cfg"),
            data_dir: PathBuf::from("/data"),
        };

        assert_eq!(paths.metadata_db_path(), PathBuf::from("/data/metadata.db"));
        assert_eq!(paths.bundled_manifest_path(), PathBuf::from("/cfg/bundled/manifest.json"));
        assert_eq!(paths.standard_required_dirs().len(), 15);
    }
}
