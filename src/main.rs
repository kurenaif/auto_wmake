extern crate regex;

use std::io::Read;
use std::path::Path;
use std::fs::{self, metadata, File};
use std::error::Error;
use std::env;
use std::ffi::OsStr;
use std::collections::LinkedList;
use std::collections::{HashMap, HashSet};

use regex::Regex;

fn get_make_target(dir_name: &Path) -> Option<(String, String)> {
    let path = Path::new(dir_name).join("Make/files");
    let display = path.display();

    let mut file = match File::open(&path) {
        Err(why) => panic!("couldn't open {}: {}", display, Error::description(&why)),
        Ok(file) => file,
    };

    let mut s = String::new();
    let contents = match file.read_to_string(&mut s) {
        Err(why) => panic!("couldn't read {}: {}", display, Error::description(&why)),
        Ok(_) => s,
    };

    for line in contents.as_str().split('\n') {
        let line = str::replace(line, " ", "");
        let mut split_line = line.split('=');
        let first = split_line.next();
        let second = split_line.next();
        if second != None {
            let first = first.unwrap().to_string();
            let second: String = Path::new(second.unwrap())
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
                .chars()
                .skip(3)
                .collect();
            if first == "LIB" || first == "EXE" {
                return Some((first, second));
            }
        }
    }
    None
}

fn get_dependencies(dir_name: &Path) -> Vec<String> {
    let mut res: Vec<String> = Vec::new();

    let path = Path::new(dir_name).join("Make/options");
    let display = path.display();

    let mut file = match File::open(&path) {
        Err(why) => panic!("couldn't open {}: {}", display, Error::description(&why)),
        Ok(file) => file,
    };

    let mut s = String::new();
    let contents = match file.read_to_string(&mut s) {
        Err(why) => panic!("couldn't read {}: {}", display, Error::description(&why)),
        Ok(_) => s,
    };

    let re = Regex::new(r"\\(?s:.)").unwrap();
    let contents = re.replace_all(&contents, "");

    for line in contents.split('\n') {
        let mut split_line = line.split('=').clone();
        let dependency_type = str::replace(split_line.next().unwrap(), " ", "");
        let second = split_line.next();
        if second != None {
            let second = second.unwrap();
            if dependency_type == "EXE_LIBS" || dependency_type == "LIB_LIBS" {
                for file in second.split_whitespace() {
                    let string_file: String = file.to_string().chars().skip(2).collect();
                    res.push(string_file)
                }
            }
        }
    }

}
    res

fn walk_dir(dir: &Path) -> LinkedList<String> {
    let mut res: LinkedList<String> = LinkedList::new();

    match fs::read_dir(dir) {
        Err(why) => println!("! {:?}", why.kind()),
        Ok(paths) => for path in paths {
            let path = path.unwrap();
            if path.file_name() == OsStr::new("Make") {
                res.push_back(dir.to_str().unwrap().to_string());
                break;
            }
            let nxt = String::from(path.path().to_str().unwrap());
            let nxt = Path::new(&nxt);
            if metadata(nxt).unwrap().is_dir() {
                res.append(&mut walk_dir(nxt));
            }
        },
    }

    res
}

fn get_edges(root_dir: &Path, memo: &mut HashSet<String>) -> LinkedList<(String, String)> {
    let mut res: LinkedList<(String, String)> = LinkedList::new();

    let src_dir = env::var("FOAM_SRC").unwrap();
    let make_candidate_dirs = walk_dir(Path::new(&src_dir));
    let mut library_dirs = HashMap::new();
    for dir in make_candidate_dirs {
        library_dirs.insert(get_make_target(Path::new(&dir)).unwrap().1, dir);
    }
    for dir in get_dependencies(root_dir) {
        match library_dirs.get(&dir) {
            Some(nxt) => {
                res.push_back((root_dir.to_string_lossy().into_owned(), nxt.to_string()));
                if !memo.contains(&nxt.to_string()) {
                    res.append(&mut get_edges(Path::new(nxt), memo));
                }
                memo.insert(nxt.to_string());
            }
            None => continue,
        }
    }

    res
}

fn main() {
    let edges = get_edges(
        Path::new(
            "/home/kurenaif/OpenFOAM/OpenFOAM-dev/applications/solvers/incompressible/pimpleFoam",
        ),
        &mut HashSet::new(),
    );
    println!("{:?}", edges);
    let mut graph : HashMap<String, Vec<String>> = HashMap::new();
    for edge in edges {
        if !graph.contains_key(&edge.0) {
            graph.insert(edge.0.clone(), Vec::new());
        }
        graph.get_mut(&edge.0).unwrap().push(edge.1);
    }
}
