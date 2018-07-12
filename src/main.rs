extern crate regex;
extern crate clap;
extern crate strsim;

use std::io::Read;
use std::path::{Path};
use std::fs::{self, metadata, File};
use std::error::Error;
use std::env;
use std::ffi::OsStr;
use std::collections::{LinkedList, HashMap, HashSet, VecDeque};
use std::process::{Command, Stdio};
use strsim::levenshtein;
use clap::{App, Arg};
use regex::Regex;

// get make target from Make/file
// e.g.) LIB = $(FORM_LIBBIN)/libsurfaceFilmModels => surfaceFilmModels
fn get_make_target(dir_name: &Path) -> Option<(String, String)> {
    let path = Path::new(dir_name).join("Make/files");
    let display = path.display();

    let mut file = match File::open(&path) {
        Err(why) => {eprintln!("warning: couldn't open {}: {}", display, Error::description(&why)); return None}
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
            let second = match &*first {
                "LIB" => 
                Path::new(second.unwrap())
                    .file_name() // get /xxxx/xxx/xxx/"file_name"
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
                    .chars()
                    .skip(3) // skip "lib"xxxxxx
                    .collect(),
                "EXE" => 
                Path::new(second.unwrap())
                    .file_name() // get /xxxx/xxx/xxx/"file_name"
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
                    .chars()
                    .collect(),
                _  => 
                    continue
            };
            if first == "LIB" || first == "EXE" {
                return Some((first, second));
            }
        }
    }
    None
}

// get library dependencies from Make/option file
// return the directory depend on `dir_name` for wmake.
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
    res
}

// List directries under dir given by argument.
// like `find -type d` command
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
            match metadata(nxt) {
                Ok(temp) => if temp.is_dir() {res.append(&mut walk_dir(nxt));},
                Err(err) => {},
            }
        },
    }

    res
}

// make edges from dependencies.
// `memo` argument prevent infinite recursion.
// `src_dir` is explorer directory. for find library dirctories.
// `root_dir` is the directory you want to wmake in the end.
fn get_edges(root_dir: &Path, memo: &mut HashSet<String>, src_dir: &String) -> LinkedList<(String, String)> {
    let mut res: LinkedList<(String, String)> = LinkedList::new();

    let make_candidate_dirs = walk_dir(Path::new(&src_dir));
    let mut library_dirs = HashMap::new();
    for dir in make_candidate_dirs {
        match get_make_target(Path::new(&dir)) {
            Some(target) => {library_dirs.insert(target.1, dir);}
            None => eprintln!("warning: {}/Make/files not found.", dir)
        }
        
    }
    for dir in get_dependencies(root_dir) {
        match library_dirs.get(&dir) {
            Some(nxt) => {
                res.push_back((root_dir.to_string_lossy().into_owned(), nxt.to_string()));
                if !memo.contains(&nxt.to_string()) {
                    res.append(&mut get_edges(Path::new(nxt), memo, src_dir));
                }
                memo.insert(nxt.to_string());
            }
            None => continue,
        }
    }

    res
}

// get 0 in degree for initial queue. (if some directory in degree is 0, the directory can wmake unconditionally)
fn get_zero_in_degree(in_degree: &HashMap<String, i32>) -> VecDeque<String> {
    let mut queue = VecDeque::new();
    for (node, degree) in in_degree {
        if *degree == 0 {
            queue.push_back(node.to_string());
        }
    }
    queue
}

// standard output dot file for generate graph.
fn output_dot_graph(graph: &HashMap<String, Vec<String>>){
    println!("digraph dependensy{{");
    let base_dir = env::var("WM_PROJECT_DIR").unwrap();
    let base_path = Path::new(&base_dir);

    for (from, tos) in graph {
        for to in tos {
            let from_path = Path::new(&from);
            let to_path = Path::new(&to);
            println!("\"{}\"->\"{}\"", from_path.strip_prefix(base_path).unwrap().to_str().unwrap(), to_path.strip_prefix(base_path).unwrap().to_str().unwrap());
        }
    }
    println!("}}");
}

// wmake target
// jobs_string: -jn
// is_stdout_detail: is output wmake stream message
fn wmake(target: &str, jobs_string: &str, is_stdout_detail: bool){
    let mut cmd = Command::new("wmake")
        .arg(&jobs_string)
        .current_dir(&target)
        .stdout(if is_stdout_detail {Stdio::inherit() } else { Stdio::null() })
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let status = cmd.wait().expect(&("failed to wmake ".to_owned() + &target));
    if status.code().unwrap() != 0 {
        panic!("failed to wmake {}", target);
    }
    println!("Ok {}", status);
}

// check path and search path from app name. if can't find path, reccomend app.
fn check_recommend(target: &str) -> Result<String, String> {
    let make_path = Path::new(target).join("Make"); // make path
    if make_path.exists(){
        return Ok(String::from(target));
    }

    let mut min_distance = (usize::max_value(), String::from(""), String::from(""));
    if !make_path.exists() {
        let make_candidate_dirs = walk_dir(Path::new(&env::var("FOAM_APP").unwrap()));
        for dir in make_candidate_dirs {
            let temp = get_make_target(Path::new(&dir)).unwrap();
            let dist = levenshtein(&temp.1, target);
            if min_distance.0 > dist {
                min_distance = (dist, String::from(temp.1), String::from(dir));
            }
        }
    }

    if min_distance.0 == 0{
        Ok(min_distance.2)
    }
    else{
        Err("Error app/Path \"".to_owned() + target + "\" is not found. Did you mean \"" + min_distance.1.as_str() + "\"?")
    }
}

// initial build for directory which is difficult to make.
fn init_build(jobs_string: &str, is_stdout_detail: bool){
    println!("make {}", env::var("WM_PROJECT_DIR").unwrap()+"/wmake/src");
    let mut cmd = Command::new("make")
        .arg(&jobs_string)
        .current_dir(env::var("WM_PROJECT_DIR").unwrap()+"/wmake/src")
        .stdout(if is_stdout_detail {Stdio::inherit() } else { Stdio::null() })
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let status = cmd.wait().expect(&("failed to make wmake"));
    if status.code().unwrap() != 0 {
        panic!("failed to make wmake");
    }

    println!("Allwmake {}", env::var("FOAM_SRC").unwrap() + "/Pstream");
    let mut cmd = Command::new("./Allwmake")
        .arg(&jobs_string)
        .current_dir(env::var("FOAM_SRC").unwrap() + "/Pstream")
        .stdout(if is_stdout_detail {Stdio::inherit() } else { Stdio::null() })
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let status = cmd.wait().expect(&("failed to Allwmake Pstream"));
    if status.code().unwrap() != 0 {
        panic!("failed to make wmake");
    }

    println!("{}", env::var("FOAM_SRC").unwrap() + "/OSspecific/" + env::var("WM_OSTYPE").unwrap_or("POSIX".to_owned()).as_str());
    let mut cmd = Command::new("./Allwmake")
        .arg(&jobs_string)
        .current_dir(env::var("FOAM_SRC").unwrap() + "/OSspecific/" + env::var("WM_OSTYPE").unwrap_or("POSIX".to_owned()).as_str())
        .stdout(if is_stdout_detail {Stdio::inherit() } else { Stdio::null() })
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let status = cmd.wait().expect(&("failed to Allwmake OSspecific"));
    if status.code().unwrap() != 0 {
        panic!("failed to make OSspecific");
    }
}

fn main() {
    let matches = App::new("auto_wmake")
        .version("0.1")
        .author("kurenaif <antyobido@gmail.com>")
        .about("OpenFOAM wmake right product at the right time.")
        .arg(Arg::with_name("path/app")
            .help("Build directory path or application name (in FOAM_APP). If omitted, the current directory is applied.")
            .index(1)
            .required(true))
        .arg(Arg::with_name("detail")
            .short("d")
            .long("detail")
            .help("Output wmake message in detail"))
        .arg(Arg::with_name("graph")
            .short("g")
            .long("graph")
            .help("output dependency graph"))
        .arg(Arg::with_name("jobs")
            .short("j")
            .long("jobs")
            .value_name("N")
            .help("allow several jobs at once")
            .takes_value(true))
        .arg(Arg::with_name("skip-init")
            .short("s")
            .long("skip-init")
            .help("skip initial make"))
        .get_matches();

    let is_stdout_detail = if matches.is_present("detail") { true }  else { false };
    let is_output_dependency_graph = if matches.is_present("graph") { true }  else { false };
    let jobs_number = match matches.value_of("jobs").unwrap_or("1").parse::<i32>() {
        Ok(num) => num,
        Err(_) => {eprintln!("job number is not invalid"); std::process::exit(1)}
    };
    let jobs_string = if jobs_number != 1 {
        "-j".to_owned() + &jobs_number.to_string()
    } else {
        "".to_owned()
    };


    let src_dir = match env::var("FOAM_SRC") {
        Ok(dir) => dir,
        Err(_) => {eprintln!("OpenFOAM-***/etc/basrhc is not read. execute `source OpenFOAM-***/etc/bashrc`"); std::process::exit(1)}
    };

    let arg_path = match check_recommend(matches.value_of("path/app").unwrap_or(".")){
        Ok(path) => path,
        Err(message) => {eprintln!("{}", message); std::process::exit(1)}
    };

    let edges = get_edges(
        Path::new(
            &arg_path,
        ),
        &mut HashSet::new(),
        &src_dir
    );
    let mut graph : HashMap<String, Vec<String>> = HashMap::new();
    let mut in_degree : HashMap<String, i32> = HashMap::new();

    for edge in edges {
        in_degree.entry(edge.1.clone()).or_insert(0);
        let deg = in_degree.entry(edge.0.clone()).or_insert(0);
        *deg += 1;
        graph.entry(edge.0.clone()).or_insert(Vec::new());
        let from = graph.entry(edge.1.clone()).or_insert(Vec::new());
        from.push(edge.0);
    }

    if is_output_dependency_graph {
        output_dot_graph(&graph);
        return;
    }

    if !matches.is_present("skip-init") {
        init_build(jobs_string.as_str(), is_stdout_detail);
    }

    // wmake queue.
    let mut queue = get_zero_in_degree(&in_degree);
    // number to wmake
    let size = graph.len();

    // number of wmake completed.
    let mut cnt = 0;
    while let Some(target) = queue.pop_front() {
        cnt += 1;
        // output progress
        println!("[{}/{}] {}", cnt, size, target);
        wmake(&target, &jobs_string, is_stdout_detail);
        let nexts = graph.get_mut(&target).unwrap();
        for nxt in nexts {
            *in_degree.get_mut(nxt).unwrap() -= 1;
            if in_degree.get(nxt).unwrap() == &0 {
                queue.push_back(nxt.to_string());
            }
        }
    }
}
