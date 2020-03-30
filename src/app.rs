use lmtreedb::path::{Path, PathPart, Root};
use lmtreedb::{DataWrapperV1, Storage};
use my_error::*;

#[derive(Debug)]
struct Autocomplete {
    position: usize,
    possible: Vec<String>,
    original: String,
    correct: bool,
}

impl Autocomplete {
    fn new(input: &str) -> Self {
        let mut possible = Self::find_possible(input);
        let correct = possible.len() == 1 && possible[0] == input;
        if correct {
            possible.clear();
        }
        let mut res = Self {
            possible,
            correct,
            position: 0,
            original: input.to_string(),
        };
        res.next();
        res
    }

    fn find_possible(input: &str) -> Vec<String> {
        let splitted: Vec<&str> = input.split(' ').collect();
        if splitted.is_empty() {
            Self::commands("")
        } else if splitted.len() == 1 {
            Self::commands(&splitted[0])
        } else {
            vec![]
        }
    }

    fn commands(input: &str) -> Vec<String> {
        let cmds = [
            "cd", "ls", "rm", "read", "help", "quit", "exit", "dbg", "write",
        ];
        let mut res = Vec::with_capacity(cmds.len());
        for i in &cmds {
            if i.starts_with(input) {
                res.push(i.to_string());
            }
        }
        res
    }

    fn next(&mut self) {
        self.position += 1;
        self.position %= self.possible.len() + 1;
    }

    fn current(&self) -> &str {
        if self.position == 0 {
            &self.original
        } else {
            &self.possible[self.position - 1]
        }
    }

    fn reset(self) -> String {
        self.original
    }

    fn highlight(&self) -> bool {
        self.correct || self.position != 0
    }
}

#[derive(Debug)]
struct FileBrowser {
    files: Vec<String>,
    path: Path,
    info: String,
    info_title: String,
    file_info: String,
    selected: usize,
    storage: Storage,
}

#[derive(Debug)]
pub enum AppState {
    Running,
    Stopped,
}

#[derive(Clone)]
enum CdPath {
    Relative(Path),
    Absolute(Path),
    Auto(String),
    Selected,
    Up,
    Current,
}

impl CdPath {
    fn non_auto(self) -> CdPath {
        match self {
            CdPath::Auto(auto) => {
                let path: Vec<&str> = auto.split('/').collect();
                match &path[..] {
                    ["."] => CdPath::Current,
                    [".."] => CdPath::Up,
                    _ => {
                        let root = Root::default().path().to_string();
                        let p = Path(path.into_iter().map(|x| x.to_string()).collect());
                        if auto.starts_with('/') || auto.starts_with(&root) {
                            CdPath::Absolute(p)
                        } else {
                            CdPath::Relative(p)
                        }
                    }
                }
            }
            other => other,
        }
    }
}

impl FileBrowser {
    fn decrease(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    fn increase(&mut self) {
        if self.selected + 1 < self.files.len() {
            self.selected += 1;
        }
    }

    fn cd(&mut self, path: CdPath) -> Result<(), Error> {
        let orig = self.path.clone();
        match path.non_auto() {
            CdPath::Auto(_) => unreachable!(),
            CdPath::Relative(rel) => self.path += rel,
            CdPath::Absolute(abs) => self.path = abs,
            CdPath::Up => {
                if self.path.0.len() != 1 {
                    self.path.0.pop();
                }
            }
            CdPath::Selected => {
                return self.cd(self.selected_path().0);
            }
            CdPath::Current => {}
        }
        if let Err(e) = self.ls().epos(pos!()) {
            self.path = orig;
            return Err(e);
        };
        self.selected = 0;
        Ok(())
    }

    fn ls(&mut self) -> Result<(), Error> {
        let info = self.storage.children(self.path.clone()).epos(pos!())?;
        let info: DataWrapperV1 = info.err(pos!())?;

        self.file_info = format!(
            "child count: {}\nversion: {}",
            info.children.len(),
            info.version
        );

        let mut files: Vec<String> = info.children.into_iter().map(|x| x.to_string()).collect();
        files.sort();
        self.files = vec![".".to_string(), "..".to_string()];
        self.files.append(&mut files);

        Ok(())
    }

    fn selected_path(&self) -> (CdPath, Path) {
        let new_path = self.files.get(self.selected);
        let res = if let Some(p) = new_path {
            if p == "." {
                CdPath::Current
            } else if p == ".." {
                CdPath::Up
            } else {
                let p = p.clone();
                CdPath::Relative(Path(vec![p]))
            }
        } else {
            CdPath::Current
        };
        let resolved = match res.clone().non_auto() {
            CdPath::Relative(rel) => self.path.clone() + rel,
            CdPath::Absolute(abs) => abs,
            CdPath::Auto(_) => unreachable!(),
            CdPath::Selected => unreachable!(),
            CdPath::Up => self.path.pop().0,
            CdPath::Current => self.path.clone(),
        };
        (res, resolved)
    }

    fn write(&mut self, path: CdPath) -> Result<(), Error> {
        let path = match path.non_auto() {
            CdPath::Auto(_) => unreachable!(),
            CdPath::Relative(rel) => self.path.clone() + rel,
            CdPath::Absolute(abs) => abs,
            CdPath::Up => self.path.pop().0,
            CdPath::Selected => {
                return self.write(self.selected_path().0);
            }
            CdPath::Current => self.path.clone(),
        };
        self.storage.put(path, ()).epos(pos!())?;
        self.ls().epos(pos!())?;
        Ok(())
    }

    fn rm(&mut self) -> Result<(), Error> {
        let path = self.selected_path();
        match path.0 {
            CdPath::Relative(_) | CdPath::Absolute(_) => {
                self.storage.del(path.1).epos(pos!())?;
            }
            CdPath::Current | CdPath::Up => {
                return Err(err!("Cannot remove current or parent file"))
            }
            CdPath::Selected | CdPath::Auto(_) => unreachable!(),
        }
        self.ls().epos(pos!())?;
        Ok(())
    }

    fn read_dbg(&mut self) -> Result<(), Error> {
        let path = self.selected_path().1;
        let info = self.storage.children(path).epos(pos!())?;
        let info: DataWrapperV1 = info.err(pos!())?;

        self.info_title = self.path.to_string();
        self.info = format!("{:#?}", info.data);
        Ok(())
    }
}

#[derive(Debug)]
pub struct App {
    input: String,
    autocomplete: Option<Autocomplete>,
    error_occurred: bool,
    browser: FileBrowser,
    pub state: AppState,
}

impl App {
    pub fn info(&self) -> &str {
        &self.browser.info
    }
    pub fn short_info(&self) -> &str {
        &self.browser.file_info
    }
    pub fn info_title(&self) -> &str {
        &self.browser.info_title
    }
    pub fn file_list(&self) -> Vec<String> {
        self.browser.files.iter().map(|x| x.to_string()).collect()
    }
    pub fn selected(&self) -> Option<usize> {
        let sel: usize = self.browser.selected;
        if sel < self.browser.files.len() {
            Some(sel)
        } else {
            None
        }
    }
    pub fn path(&self) -> String {
        self.browser.path.to_string()
    }
    pub fn current(&self) -> &str {
        &self.input
    }
    pub fn up(&mut self) {
        self.browser.decrease();
    }
    pub fn down(&mut self) {
        self.browser.increase();
    }
    pub fn right(&mut self) {
        let res = self.browser.cd(CdPath::Selected);
        self.show_error(res);
    }
    pub fn left(&mut self) {
        let res = self.browser.cd(CdPath::Up);
        self.show_error(res);
    }
    fn show_error(&mut self, err: Result<(), Error>) {
        if let Err(e) = err {
            self.browser.info = e.to_string();
            self.browser.info_title = "Error".to_string();
        }
    }
    pub fn enter(&mut self) {
        self.error_occurred = false;
        self.autocomplete = None;

        let input: String = self.input.drain(..).collect();

        if input.is_empty() {
            self.right();
            return;
        }

        let mut splitted: Vec<&str> = input.split(' ').collect();
        if splitted.is_empty() {
            return;
        }
        splitted.reverse();
        let cmd = splitted.pop().unwrap();
        match (cmd, splitted.len()) {
            ("cd", 1) => {
                let res = self
                    .browser
                    .cd(CdPath::Auto(splitted.pop().unwrap().to_string()));
                self.show_error(res);
            }
            ("ls", 0) => {
                let res = self.browser.ls();
                self.show_error(res);
            }
            ("rm", 0) => {
                let res = self.browser.rm();
                self.show_error(res);
            }
            ("write", 1) => {
                let res = self
                    .browser
                    .write(CdPath::Auto(splitted.pop().unwrap().to_string()));
                self.show_error(res);
            }
            ("read", 0) => {
                let res = self.browser.read_dbg();
                self.show_error(res);
            }
            ("dbg", 0) => {
                self.browser.info = format!("{:#?}", self);
                self.browser.info_title = "debug".to_string()
            }
            ("read", 1) => {}
            ("help", 0) => {
                self.browser.info_title = "Help".to_string();
                self.browser.info = String::from(concat!(
                "Command line:",
                "\n    `ls` — update current directory",
                "\n    `cd <path>` — change path",
                "\n    `write <name>` — creates empty file",
                "\n    `rm` — removes selected file",
                "\n    `read` — debug print selected file",
                "\n    `read <schema>` — read and parse current path. Shows more info in Info window",
                "\n    `exit` | `quit` — exit",
                "\nKeys:",
                "\n    <Ctrl>+<D> | <F10> | <ESC> — exit",
                "\n    <TAB> to autocomplete",
                "\n    <UP> and <DOWN> to navigate file list",
                "\n    <RIGHT> to change path to selected",
                "\n    <LEFT> to go up"
                ))
            }
            ("exit", 0) | ("quit", 0) => self.state = AppState::Stopped,
            _ => {
                self.error_occurred = true;
                self.input = input;
            }
        }
    }
    pub fn ctrlc(&mut self) {
        self.error_occurred = false;
        if let Some(auto) = self.autocomplete.take() {
            self.input = auto.reset();
        } else {
            self.input.clear();
        }
    }
    pub fn backspace(&mut self) {
        self.error_occurred = false;
        self.autocomplete = None;
        self.input.pop();
    }
    pub fn key(&mut self, k: char) {
        self.error_occurred = false;
        self.autocomplete = None;
        self.input.push(k);
    }
    pub fn tab(&mut self) {
        self.error_occurred = false;
        match &mut self.autocomplete {
            None => {
                let auto = Autocomplete::new(&self.input);
                self.autocomplete = Some(auto);
            }
            Some(auto) => {
                auto.next();
            }
        };
        if let Some(auto) = &self.autocomplete {
            self.input = auto.current().to_string();
        }
    }
    pub fn color(&self) -> tui::style::Color {
        match &self.autocomplete {
            None => match self.error_occurred {
                true => tui::style::Color::Red,
                false => tui::style::Color::White,
            },
            Some(auto) => match auto.highlight() {
                true => tui::style::Color::Yellow,
                false => tui::style::Color::White,
            },
        }
    }
    pub fn stop(&mut self) {
        self.state = AppState::Stopped;
    }

    pub fn connect(path: &std::path::Path) -> Result<Self, Error> {
        let mut res = App {
            input: String::new(),
            autocomplete: None,
            error_occurred: false,
            browser: FileBrowser {
                path: Root::default().path(),
                selected: 0,
                file_info: String::new(),
                info: String::new(),
                info_title: "Info".to_string(),
                files: Vec::new(),
                storage: Storage::connect(path)?,
            },
            state: AppState::Running,
        };
        res.browser
            .cd(CdPath::Absolute(Root::default().path()))
            .epos(pos!())?;
        Ok(res)
    }
}
