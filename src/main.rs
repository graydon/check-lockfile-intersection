use attohttpc::get;
use cargo_lock::{Lockfile, Name, Package};
use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
};
use url::Url;

#[derive(Default)]
struct Spec {
    src: String,
    pkg_name: Option<String>,
    pkg_hash: Option<String>,
    exclude_pkgs: BTreeSet<String>,
}

#[derive(Default)]
struct Args {
    spec_a: Spec,
    spec_b: Spec,
}

impl Args {
    fn parse() -> Result<Self, String> {
        let args = std::env::args().collect::<Vec<String>>();
        let mut iter = args.iter().skip(1);
        let mut res = Args::default();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--pkg-hash-a" => {
                    res.spec_a.pkg_hash = iter.next().cloned();
                }
                "--pkg-hash-b" => {
                    res.spec_b.pkg_hash = iter.next().cloned();
                }
                "--pkg-name-a" => {
                    res.spec_a.pkg_name = iter.next().cloned();
                }
                "--pkg-name-b" => {
                    res.spec_b.pkg_name = iter.next().cloned();
                }
                "--exclude-pkg-a" => {
                    res.spec_a
                        .exclude_pkgs
                        .insert(iter.next().cloned().unwrap());
                }
                "--exclude-pkg-b" => {
                    res.spec_b
                        .exclude_pkgs
                        .insert(iter.next().cloned().unwrap());
                }
                _ => {
                    if res.spec_a.src.is_empty() {
                        res.spec_a.src = arg.clone();
                    } else if res.spec_b.src.is_empty() {
                        res.spec_b.src = arg.clone();
                    } else {
                        return Err(format!("Unexpected argument: {}", arg));
                    }
                }
            }
        }
        if res.spec_a.src.is_empty() || res.spec_b.src.is_empty() {
            return Err("Missing lockfile source arguments".to_string());
        }
        Ok(res)
    }
}

struct State {
    spec: Spec,
    lockfile: Lockfile,
    packages: BTreeMap<Name, Package>,
}

fn load_lockfile(src: &str) -> Result<Lockfile, String> {
    let url = match Url::parse(src) {
        Ok(url) => url,
        Err(url::ParseError::RelativeUrlWithoutBase) => {
            // Possibly we were just given a plain file path, try that.
            return Lockfile::load(src).map_err(|e| e.to_string());
        }
        Err(e) => return Err(e.to_string()),
    };
    if url.scheme() == "file" {
        let path = url
            .to_file_path()
            .map_err(|_| "file URL error".to_string())?;
        return Lockfile::load(path).map_err(|e| e.to_string());
    } else if url.scheme() == "http" || url.scheme() == "https" {
        let response = get(url.as_str()).send().map_err(|e| e.to_string())?;
        if !response.is_success() {
            return Err(format!(
                "Failed to fetch lockfile: {}",
                response.text().unwrap_or_default()
            ));
        }
        let text = response.text().map_err(|e| e.to_string())?;
        return Lockfile::from_str(&text).map_err(|e| e.to_string());
    } else {
        return Err(format!("Unsupported URL scheme: {}", url.scheme()));
    }
}

fn package_matches_hash(pkg: &cargo_lock::Package, hash: &str) -> bool {
    // Try comparing hash to hashes in either the package checksum or the source
    // precise field
    if let Some(cksum) = &pkg.checksum {
        if cksum.to_string() == hash {
            return true;
        }
    }
    if let Some(src) = &pkg.source {
        if let Some(precise) = src.precise() {
            if precise == hash {
                return true;
            }
        }
    }
    false
}

impl State {
    fn new(spec: Spec) -> Result<Self, String> {
        let lockfile = load_lockfile(&spec.src)?;
        Ok(State {
            spec,
            lockfile,
            packages: BTreeMap::new(),
        })
    }

    fn try_insert_package(&mut self, package: &Package) -> Result<bool, String> {
        if let Some(existing) = self.packages.get(&package.name) {
            if existing.version != package.version {
                return Err(format!(
                    "Package {} has multiple versions in lockfile {}: {} and {}",
                    package.name, self.spec.src, existing.version, package.version
                ));
            }
            Ok(false)
        } else {
            println!("{} {} {}", self.spec.src, package.name, package.version);
            self.packages.insert(package.name.clone(), package.clone());
            Ok(true)
        }
    }

    fn add_all_dependencies_recursive(&mut self, package: &Package) -> Result<(), String> {
        for dep in package.dependencies.iter() {
            if self.spec.exclude_pkgs.contains(dep.name.as_str()) {
                continue;
            }
            let dep_pkg = self
                .lockfile
                .packages
                .iter()
                .cloned()
                .find(|p| dep.matches(p));
            if let Some(dep_pkg) = dep_pkg {
                if self.try_insert_package(&dep_pkg)? {
                    self.add_all_dependencies_recursive(&dep_pkg)?;
                }
            }
        }
        Ok(())
    }

    fn add_packages_in_dependency_tree(&mut self) -> Result<(), String> {
        let all_packages = self.lockfile.packages.iter().cloned().collect::<Vec<_>>();
        for package in all_packages.iter() {
            if self.spec.exclude_pkgs.contains(package.name.as_str()) {
                continue;
            }
            if let Some(name) = &self.spec.pkg_name {
                if package.name.as_str() != *name {
                    continue;
                }
            }
            if let Some(hash) = &self.spec.pkg_hash {
                if !package_matches_hash(package, hash) {
                    continue;
                }
            }

            self.packages.insert(package.name.clone(), package.clone());
            self.add_all_dependencies_recursive(package)?;
            return Ok(());
        }
        Err(format!(
            "No package named {:?} with hash {:?} found in lockfile {}",
            self.spec.pkg_name, self.spec.pkg_hash, self.spec.src
        ))
    }

    fn add_all_packages_in_lockfile(&mut self) -> Result<(), String> {
        let all_packages = self
            .lockfile
            .packages
            .iter()
            .cloned()
            .filter(|x| !self.spec.exclude_pkgs.contains(x.name.as_str()))
            .collect::<Vec<_>>();
        for package in all_packages {
            self.try_insert_package(&package)?;
        }
        Ok(())
    }

    fn add_packages(&mut self) -> Result<(), String> {
        if self.spec.pkg_name.is_some() || self.spec.pkg_hash.is_some() {
            self.add_packages_in_dependency_tree()
        } else {
            self.add_all_packages_in_lockfile()
        }
    }
}

struct Program {
    state_a: State,
    state_b: State,
}

impl Program {
    fn new() -> Result<Self, String> {
        let args = Args::parse()?;
        let state_a = State::new(args.spec_a)?;
        let state_b = State::new(args.spec_b)?;
        Ok(Program { state_a, state_b })
    }

    fn run(&mut self) -> Result<(), String> {
        self.state_a.add_packages()?;
        self.state_b.add_packages()?;
        let package_names_a = self.state_a.packages.keys().collect::<BTreeSet<_>>();
        let package_names_b = self.state_b.packages.keys().collect::<BTreeSet<_>>();
        let intersection = package_names_a
            .intersection(&package_names_b)
            .collect::<BTreeSet<_>>();
        println!(
            "{} packages in lockfile A: {}",
            package_names_a.len(),
            self.state_a.spec.src
        );
        println!(
            "{} packages in lockfile B: {}",
            package_names_b.len(),
            self.state_b.spec.src
        );
        println!("{} packages in common:", intersection.len());
        let mut all_ok = true;
        for name in intersection {
            let vers_a = self.state_a.packages.get(*name).unwrap().version.clone();
            let vers_b = self.state_b.packages.get(*name).unwrap().version.clone();
            if vers_a == vers_b {
                println!("OK {} {}", name, vers_a);
            } else {
                println!("DIFFERENT {} {} vs. {}", name, vers_a, vers_b);
                all_ok = false;
            }
        }
        if all_ok {
            Ok(())
        } else {
            Err("Some packages have different versions".to_string())
        }
    }
}

fn main() -> Result<(), String> {
    let mut program = Program::new()?;
    program.run()
}
