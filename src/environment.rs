/// Query the current user environment
use crate::errors;
use failure::Error;
use std::env;
use std::path;
use std::process::Command;

#[derive(Debug)]
pub struct Environment {
    /// User uid and gid
    pub user_details: (String, String),
    /// The directory floki was launched in
    pub current_directory: path::PathBuf,
    /// The root directory for floki (may be different from
    /// the above if we had to search for floki.yaml
    pub floki_root: path::PathBuf,
    /// Absolute path to the configuration file
    pub config_file: path::PathBuf,
    /// Path to ssh socket if found
    pub ssh_agent_socket: Option<String>,
    /// The host folder that floki uses to e.g. create directories
    /// to back volumes
    pub floki_workspace: path::PathBuf,
}

impl Environment {
    /// Gather information on the environment floki is running in
    pub fn gather(config_file: &Option<path::PathBuf>) -> Result<Self, Error> {
        let (floki_root, config_path) = resolve_floki_root_and_config(config_file)?;
        Ok(Environment {
            user_details: get_user_details()?,
            current_directory: get_current_working_directory()?,
            floki_root: floki_root,
            config_file: normalize_path(config_path)?,
            ssh_agent_socket: get_ssh_agent_socket_path(),
            floki_workspace: get_floki_work_path()?,
        })
    }
}

/// Run a command and extract stdout as a String
fn run_and_get_raw_output(cmd: &mut Command) -> Result<String, Error> {
    let output = String::from_utf8(cmd.output()?.stdout)?;
    Ok(output.trim_end().into())
}

/// Get the user and group ids of the current user
fn get_user_details() -> Result<(String, String), Error> {
    let user = run_and_get_raw_output(Command::new("id").arg("-u"))?;
    debug!("User's current id: {:?}", user);
    let group = run_and_get_raw_output(Command::new("id").arg("-g"))?;
    debug!("User's current group: {:?}", group);
    Ok((user, group))
}

/// Get the current working directory as a String
fn get_current_working_directory() -> Result<path::PathBuf, Error> {
    Ok(env::current_dir()?)
}

/// Get the path of the ssh agent socket from the SSH_AUTH_SOCK
/// environment variable
fn get_ssh_agent_socket_path() -> Option<String> {
    match env::var("SSH_AUTH_SOCK") {
        Ok(path) => Some(path),
        Err(_) => None,
    }
}

/// Search all ancestors of the current directory for a floki.yaml file name.
fn find_floki_yaml(current_directory: &path::Path) -> Result<path::PathBuf, Error> {
    current_directory
        .ancestors()
        .map(|a| a.join("floki.yaml"))
        .find(|f| f.is_file())
        .ok_or(errors::FlokiError::ProblemFindingConfigYaml {}.into())
}

/// Take a file path, and return a tuple consisting of its parent directory and the file path
fn locate_file_in_parents(path: path::PathBuf) -> Result<(path::PathBuf, path::PathBuf), Error> {
    let dir = path
        .parent()
        .ok_or_else(|| errors::FlokiInternalError::InternalAssertionFailed {
            description: format!("config_file '{:?}' does not have a parent", &path),
        })?
        .to_path_buf();
    Ok((dir, path))
}

/// Resolve floki root directory and path to configuration file. The floki root directory
/// here is the folder in which the floki.yaml was found when no configuration file
/// is specified, and we have to search for it.
fn resolve_floki_root_and_config(
    config_file: &Option<path::PathBuf>,
) -> Result<(path::PathBuf, path::PathBuf), Error> {
    match config_file {
        Some(path) => Ok((get_current_working_directory()?, path.clone())),
        None => Ok(locate_file_in_parents(find_floki_yaml(
            &get_current_working_directory()?,
        )?)?),
    }
}

/// Resolve a directory for floki to use for user-global file (caches etc)
fn get_floki_work_path() -> Result<path::PathBuf, Error> {
    let root: path::PathBuf = env::var("HOME")
        .unwrap_or(format!("/tmp/{}/", get_user_details()?.0))
        .into();
    Ok(root.join(".floki"))
}

/// Normalize the filepath - this turns a relative path into an absolute one - to
/// do this it must locate the file in the filesystem, and hence it may fail.
fn normalize_path(path: path::PathBuf) -> Result<path::PathBuf, Error> {
    let res = std::fs::canonicalize(&path).map_err(|e| {
        errors::FlokiError::ProblemNormalizingFilePath {
            name: path.display().to_string(),
            error: e,
        }
    })?;

    Ok(res)
}

#[cfg(test)]
mod test {
    use super::*;
    use failure::format_err;
    use std::fs;
    use tempdir;

    fn touch_file(path: &path::Path) -> Result<(), Error> {
        fs::create_dir_all(
            path.parent()
                .ok_or(format_err!("Unable to take parent of path"))?,
        )?;
        fs::OpenOptions::new().create(true).write(true).open(path)?;
        Ok(())
    }

    #[test]
    fn test_find_floki_yaml_current_dir() -> Result<(), Error> {
        let tmp_dir = tempdir::TempDir::new("")?;
        let floki_yaml_path = tmp_dir.path().join("floki.yaml");
        touch_file(&floki_yaml_path)?;
        assert_eq!(find_floki_yaml(&tmp_dir.path())?, floki_yaml_path);
        Ok(())
    }

    #[test]
    fn test_find_floki_yaml_ancestor() -> Result<(), Error> {
        let tmp_dir = tempdir::TempDir::new("")?;
        let floki_yaml_path = tmp_dir.path().join("floki.yaml");
        touch_file(&floki_yaml_path)?;
        assert_eq!(
            find_floki_yaml(&tmp_dir.path().join("dir/subdir"))?,
            floki_yaml_path
        );
        Ok(())
    }

    #[test]
    fn test_find_floki_yaml_sibling() -> Result<(), Error> {
        let tmp_dir = tempdir::TempDir::new("")?;
        let floki_yaml_path = tmp_dir.path().join("src/floki.yaml");
        touch_file(&floki_yaml_path)?;
        assert!(find_floki_yaml(&tmp_dir.path().join("include")).is_err());
        Ok(())
    }
}
