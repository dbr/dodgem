use std::collections::HashMap;

use clap::{App, Arg};
use git2::Repository;

#[derive(Debug)]
struct Version {
    major: i32,
    minor: i32,
    patch: i32,
}

impl Version {
    fn parse_tag(name: &str) -> anyhow::Result<Version> {
        let matcher = regex::Regex::new(r#".*-(\d+)\.(\d+)\.(\d+)"#).expect("Invalid regex");
        let m = matcher.captures(name).unwrap();

        fn match_to_int(m: &Option<regex::Match>) -> anyhow::Result<i32> {
            match m {
                None => Err(anyhow::anyhow!("Missing group")),
                Some(m) => Ok(m.as_str().parse::<i32>()?),
            }
        }

        Ok(Version {
            major: match_to_int(&m.get(1))?,
            minor: match_to_int(&m.get(2))?,
            patch: match_to_int(&m.get(3))?,
        })
    }

    fn bump_major(&self) -> Version {
        Version {
            major: self.major + 1,
            minor: 0,
            patch: 0,
        }
    }
    fn bump_minor(&self) -> Version {
        Version {
            major: self.major,
            minor: self.minor + 1,
            patch: 0,
        }
    }
    fn bump_patch(&self) -> Version {
        Version {
            major: self.major,
            minor: self.minor,
            patch: self.patch + 1,
        }
    }

    fn version_str(&self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }
}

fn bumper(path: &str, bump_type: BumpType) -> anyhow::Result<()> {
    let repo = Repository::discover(path)?;

    // Check HEAD points to branch `main`
    if repo.head()?.resolve()?.shorthand() != Some("main") {
        return Err(anyhow::anyhow!("must be on main branch"));
    }

    let diff = repo.diff_index_to_workdir(None, None)?;
    let changes = diff.stats()?.files_changed();
    if changes > 0 {
        let diff_buf = diff.stats()?.to_buf(git2::DiffStatsFormat::SHORT, 80)?;
        let stats = diff_buf.as_str().unwrap_or("(no diff available)");
        return Err(anyhow::anyhow!(
            "Repo contains uncommited changes: {}",
            stats
        ));
    }

    // Get map of commit ID to tag-name
    let tagmap = {
        let mut map: HashMap<git2::Oid, String> = HashMap::new();
        repo.tag_foreach(|x, raw_tag_name| {
            match repo.find_tag(x) {
                Ok(tag_obj) => {
                    let target = tag_obj.target_id();
                    if let Ok(name) = String::from_utf8(raw_tag_name.into()) {
                        map.insert(target, name);
                    }
                }
                Err(_) => {}
            }
            true
        })?;
        map
    };

    // Walk commits, newest to oldest
    let mut walker = repo.revwalk()?;
    walker.push_head()?;
    walker.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME)?;

    // Find last tagged commit from current branch
    let prev_tag = walker
        .filter_map(Result::ok)
        .filter(|o| tagmap.contains_key(&o))
        .next();

    // Generate new version string
    let (old_version, next_version) = match prev_tag {
        Some(t) => {
            let version = Version::parse_tag(&tagmap[&t]).unwrap();
            let next = match bump_type {
                BumpType::major => version.bump_major(),
                BumpType::minor => version.bump_minor(),
                BumpType::patch => version.bump_patch(),
            };
            (version.version_str(), next.version_str())
        }
        None => return Err(anyhow::anyhow!("No previous tag found")),
    };

    // Update files in repo
    let repo_path = repo.workdir().expect("Repo has no working directory");
    let f = repo_path.join("package.py");
    eprintln!(
        "Updating {} from {} to {}",
        &f.to_str().unwrap_or("???"),
        &old_version,
        &next_version
    );
    let contents = std::fs::read_to_string(&f)?;
    let updated = contents.replace(&old_version, &next_version);
    std::fs::write(&f, &updated)?;

    Ok(())
}

use clap::{arg_enum, value_t_or_exit};

arg_enum! {
    #[derive(Debug)]
    #[allow(non_camel_case_types)] // Variant names are used as arg values
    pub enum BumpType {
        major,
        minor,
        patch,
    }
}

fn main() -> anyhow::Result<()> {
    let args = App::new("dodgem")
        .arg(
            Arg::with_name("type")
                .value_name("type")
                .possible_values(&BumpType::variants())
                .default_value("minor"),
        )
        .arg(
            Arg::with_name("path")
                .long("path")
                .short("p")
                .takes_value(true),
        )
        .get_matches();

    let bump_type = value_t_or_exit!(args.value_of("type"), BumpType);
    let path = args.value_of("path").unwrap_or(".");

    bumper(path, bump_type)?;

    Ok(())
}
