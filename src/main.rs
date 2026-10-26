use std::{collections::HashMap, env::args, fs::{File, exists}, io::{empty, stdout}, path::Path};

use crossterm::{cursor, execute, terminal};
use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
fn recurse_and_add_to_index(path: &Path, index: &mut HashMap<String, String>) {
    if path.is_dir() {
        for entry in path.read_dir().expect("Unable to read directory") {
            let entry = entry.expect("Unable to get directory entry");
            let path = entry.path();
            index.insert(path.file_name().unwrap().to_str().unwrap().to_string(), path.to_str().unwrap().to_string());
            recurse_and_add_to_index(&path, index);
        }
    }
}
fn get_matching_close_folders(folder_name: &str, index: &HashMap<String, String>) -> Option<Vec<String>> {
    let matcher = SkimMatcherV2::default();
    
    // Create a vector to hold matches and their scores
    let mut scored_matches: Vec<(i64, String)> = index
        .keys()
        // Compute fuzzy match score. Returns None if it's not a match at all
        .filter_map(|key| {
            matcher.fuzzy_match(key, folder_name)
                .map(|score| (score, key.clone()))
        })
        .collect();

    // If no matches found, return None immediately
    if scored_matches.is_empty() {
        return None;
    }

    // Sort by score descending (highest score/best match first)
    scored_matches.sort_by(|a, b| b.0.cmp(&a.0));

    // Strip out the scores and just keep the owned String names
    let close_matches: Vec<String> = scored_matches
        .into_iter()
        .map(|(_score, name)| name)
        .collect();

    Some(close_matches)
}
fn interactive_selector(vector:&mut Vec<String>,out:&mut String){
let mut target = stdout();
let mut input_buffer;
let mut 
execute!(
    target,
    terminal::EnterAlternateScreen,
    cursor::Hide
);
    terminal::enable_raw_mode();
execute!()


}
fn set_target_folder(folder_name: &String, index: &HashMap<String, String>,target_dir: &mut String) {
    if let Some(path) = index.get(folder_name) {
        *target_dir = path.clone();
    } else {
        let keys= index.keys().collect::<Vec<&String>>();
        let mut found= false;
        for key in keys {
            if key.contains(folder_name) {
                *target_dir= index.get(key).unwrap().clone();
                break;
            }
            else {
                *target_dir= None.unwrap_or_default();
            }
        }
        
    }
}
fn main(){
    let mut index= HashMap::new();
    let mut file:File;
    let main_path= Path::new("$HOME");
    let args = args().collect::<Vec<String>>();
    let mut folder_name:String;

    if args.len() > 2 {
        panic!("Too many arguments provided. Usage: open_d <file_name>");
    } else {
        folder_name = args[1].clone();
    }


    if !exists("$HOME/.local/index.json").unwrap() {
        let index= File::create("$HOME/.local/index.json").expect("Unable to create file");
    } else {
        let index= File::open("$HOME/.local/index.json").expect("Unable to open file");
    }
let file_content= std::fs::read_to_string("$HOME/.local/index.json").expect("Unable to read file");
if file_content.is_empty() {
    recurse_and_add_to_index(main_path, &mut index);
    let json= serde_json::to_string(&index).expect("Unable to convert to JSON");
    std::fs::write("$HOME/.local/index.json", json).expect("Unable to write to file");
} else {
    index= serde_json::from_str(&file_content).expect("Unable to parse JSON");
}
let mut target_dir= String::new();
set_target_folder(&folder_name, &index, &mut target_dir);

if target_dir.is_empty() {
    if let Some(close_matches) = get_matching_close_folders(&folder_name, &index) {
        println!("No exact match found for '{}'. Did you mean one of these?", folder_name);
        for (i, match_name) in close_matches.iter().enumerate() {
            println!("{}. {}", i + 1, match_name);
        }
    } else {
        println!("No matches found for '{}'.", folder_name);
    }
} else {
    println!("Opening folder: {}", target_dir);
    // Here you can add code to open the folder using the appropriate command for your OS


}    
}
