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
    let head = repo.head()?.resolve()?;
    if head.shorthand() != Some("master") {
        panic!("not on master branch");
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

    dbg!(&tagmap);

    // Walk commits
    let mut walker = repo.revwalk()?;
    walker.push_head()?;
    walker.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME)?;

    // To find last tagged commit from current branch
    let prev_tag = walker
        .filter_map(Result::ok)
        .filter(|o| tagmap.contains_key(&o))
        .next();
    dbg!(prev_tag);

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
        None => Err(anyhow::anyhow!("No previous tag found"))?,
    };

    dbg!(&next_version);

    let repo_path = repo.workdir().expect("Repo has no working directory");
    let f = repo_path.join("package.py");
    dbg!(&f);
    let contents = std::fs::read_to_string(&f)?;
    let updated = contents.replace(&old_version, &next_version);
    println!("{}", updated);
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
