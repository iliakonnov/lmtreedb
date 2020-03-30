use std::io;
use std::io::Write;

use termion::async_stdin;
use termion::cursor::Goto;
use termion::event::Key;
use termion::input::TermRead;
use termion::raw::IntoRawMode;
use termion::screen::AlternateScreen;
use tui::backend::TermionBackend;
use tui::layout::{Constraint, Direction, Layout};
use tui::style::{Color, Style};
use tui::widgets::{Block, Borders, Paragraph, SelectableList, Text, Widget};
use tui::Terminal;
use unicode_width::UnicodeWidthStr;

use app::{App, AppState};
use my_error::*;

mod app;

fn main() -> Result<(), Error> {
    let mut stdin = async_stdin().keys();
    let stdout = io::stdout().into_raw_mode()?;
    let stdout = AlternateScreen::from(stdout);
    let backend = TermionBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut args: Vec<String> = std::env::args().collect();
    let path = if args.len() > 1 {
        args.remove(1)
    } else {
        "database".to_string()
    };

    let mut app = App::connect(std::path::Path::new(&path))?;
    loop {
        let k = stdin.next();
        if let Some(Ok(key)) = k {
            match key {
                Key::Esc | Key::Ctrl('d') | Key::F(10) => app.stop(),
                Key::Ctrl('c') => app.ctrlc(),
                Key::Char('\n') => app.enter(),
                Key::Char('\t') => app.tab(),
                Key::Char(ch) => app.key(ch),
                Key::Up => app.up(),
                Key::Down => app.down(),
                Key::Left => app.left(),
                Key::Right => app.right(),
                Key::Backspace => app.backspace(),
                _ => {
                    std::thread::sleep(std::time::Duration::from_millis(15));
                    continue;
                }
            }
        }
        if let AppState::Stopped = app.state {
            break;
        }

        let mut cursor_position = (0, 0);
        terminal.draw(|mut f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([Constraint::Min(0), Constraint::Length(3)].as_ref())
                .split(f.size());
            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Ratio(1, 3), Constraint::Ratio(2, 3)].as_ref())
                .split(chunks[0]);
            let info_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(5), Constraint::Min(0)].as_ref())
                .split(main_chunks[1]);

            SelectableList::default()
                .block(Block::default().title(&app.path()).borders(Borders::ALL))
                .items(&app.file_list())
                .select(app.selected())
                .highlight_style(Style::default().bg(Color::White).fg(Color::Black))
                .render(&mut f, main_chunks[0]);
            Paragraph::new([Text::raw(app.short_info())].iter())
                .block(Block::default().borders(Borders::ALL).title("File info"))
                .render(&mut f, info_chunks[0]);
            Paragraph::new([Text::raw(app.info())].iter())
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(app.info_title()),
                )
                .render(&mut f, info_chunks[1]);
            Paragraph::new([Text::raw(app.current())].iter())
                .style(Style::default().fg(app.color()))
                .block(Block::default().borders(Borders::ALL).title("Command line"))
                .render(&mut f, chunks[1]);
            cursor_position = (chunks[1].x + 2, chunks[1].y + 2);
        })?;
        write!(
            terminal.backend_mut(),
            "{}",
            Goto(
                cursor_position.0 + app.current().width() as u16,
                cursor_position.1,
            )
        )?;
        io::stdout().flush().ok();

        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    Ok(())
}
