use ratatui::{prelude::*, widgets::Widget};

use crate::theme::CURRENT_THEME;

use super::Item;
use std::collections::HashSet;

pub(crate) mod log;
pub(crate) mod show;
pub(crate) mod status;

pub(crate) trait ScreenData {
    fn items<'a>(&'a self) -> Vec<Item>;
}

pub(crate) struct Screen {
    pub(crate) cursor: usize,
    pub(crate) scroll: u16,
    pub(crate) size: (u16, u16),
    capture_data: Box<dyn Fn() -> Box<dyn ScreenData>>,
    items: Vec<Item>,
    ui_lines: Vec<(usize, Item, Line<'static>)>,
    // TODO Make non-string
    collapsed: HashSet<String>,
}

impl<'a> Screen {
    pub(crate) fn new(
        size: (u16, u16),
        capture_data: Box<dyn Fn() -> Box<dyn ScreenData>>,
    ) -> Self {
        let items = capture_data().items();

        let mut screen = Self {
            cursor: 0,
            scroll: 0,
            size,
            capture_data,
            items,
            ui_lines: vec![],
            collapsed: HashSet::new(),
        };

        screen.update_ui_lines();
        screen
    }

    pub(crate) fn select_next(&mut self) {
        self.cursor = self.find_next();
        self.scroll_fit_end();
        self.scroll_fit_start();
    }

    fn scroll_fit_start(&mut self) {
        let start_line = self
            .collapsed_lines_items_iter()
            .find(|(_line, i, item, _lc)| self.selected_or_direct_ancestor(item, i))
            .map(|(line, _i, _item, _lc)| line)
            .unwrap() as u16;

        if start_line < self.scroll {
            self.scroll = start_line;
        }
    }

    fn selected_or_direct_ancestor(&self, item: &Item, i: &usize) -> bool {
        let levels_above = self.get_selected_item().depth.saturating_sub(item.depth);
        i == &(self.cursor - levels_above)
    }

    fn scroll_fit_end(&mut self) {
        let depth = self.items[self.cursor].depth;
        let last = 1 + self
            .collapsed_lines_items_iter()
            .skip_while(|(_line, i, _item, _lc)| i < &self.cursor)
            .take_while(|(_line, i, item, _lc)| i == &self.cursor || depth < item.depth)
            .map(|(line, _i, _item, lc)| line + lc)
            .last()
            .unwrap();

        let end_line = self.size.1.saturating_sub(1);
        if last as u16 > end_line + self.scroll {
            self.scroll = last as u16 - end_line;
        }
    }

    pub(crate) fn find_next(&mut self) -> usize {
        self.collapsed_items_iter()
            .find(|(i, item)| i > &self.cursor && !item.unselectable)
            .map(|(i, _item)| i)
            .unwrap_or(self.cursor)
    }

    pub(crate) fn select_previous(&mut self) {
        self.cursor = self
            .collapsed_items_iter()
            .filter(|(i, item)| i < &self.cursor && !item.unselectable)
            .last()
            .map(|(i, _item)| i)
            .unwrap_or(self.cursor);

        self.scroll_fit_start();
    }

    pub(crate) fn scroll_half_page_up(&mut self) {
        let half_screen = self.size.1 / 2;
        self.scroll = self.scroll.saturating_sub(half_screen);
    }

    pub(crate) fn scroll_half_page_down(&mut self) {
        let half_screen = self.size.1 / 2;
        self.scroll = (self.scroll + half_screen).min(
            // FIXME Why doesn't this work?
            self.collapsed_lines_items_iter()
                .map(|(line, _i, _item, lc)| line + lc)
                .last()
                .unwrap_or(0) as u16,
        );
    }

    fn collapsed_lines_items_iter(&'a self) -> impl Iterator<Item = (usize, usize, &Item, usize)> {
        self.collapsed_items_iter().scan(0, |lines, (i, item)| {
            let line = *lines;
            let lc = item.display.0.lines().count();
            *lines += lc;

            Some((line, i, item, lc))
        })
    }

    pub(crate) fn toggle_section(&mut self) {
        let selected = &self.items[self.cursor];

        if selected.section {
            if self.collapsed.contains(&selected.display.0) {
                self.collapsed.remove(&selected.display.0);
            } else {
                self.collapsed.insert(selected.display.0.clone());
            }
        }

        self.update_ui_lines();
    }

    pub(crate) fn clamp_cursor(&mut self) {
        self.cursor = self.cursor.clamp(0, self.items.len().saturating_sub(1))
    }

    pub(crate) fn update(&mut self) {
        self.items = (self.capture_data)().items();
        self.update_ui_lines();
    }

    pub(crate) fn collapsed_items_iter(&'a self) -> impl Iterator<Item = (usize, &Item)> {
        self.items
            .iter()
            .enumerate()
            .scan(None, |collapse_depth, (i, next)| {
                if collapse_depth.is_some_and(|depth| depth < next.depth) {
                    return Some(None);
                }

                *collapse_depth = if next.section && self.collapsed.contains(&next.display.0) {
                    Some(next.depth)
                } else {
                    None
                };

                Some(Some((i, next)))
            })
            .flatten()
    }

    pub(crate) fn is_collapsed(&self, item: &Item) -> bool {
        self.collapsed.contains(&item.display.0)
    }

    pub(crate) fn get_selected_item(&self) -> &Item {
        &self.items[self.cursor]
    }

    fn update_ui_lines(&mut self) {
        self.ui_lines = self
            .collapsed_items_iter()
            .map(|(i, item)| (i, item, get_display_text(&item)))
            .flat_map(|(i, item, text)| {
                text.lines
                    .into_iter()
                    .map(move |line| (i, item.to_owned(), line))
            })
            .map(|(i, item, mut line)| {
                if self.is_collapsed(&item) {
                    if line.width() > 0 {
                        line.spans.push("…".into());
                    }
                }

                (i, item, line)
            })
            .collect();
    }
}

impl Widget for &Screen {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut highlight_depth = None;

        for (line_i, (item_i, item, line)) in self
            .ui_lines
            .iter()
            .skip(self.scroll as usize)
            .take(area.height as usize)
            .enumerate()
        {
            if self.cursor == *item_i {
                highlight_depth = Some(item.depth);
            } else if highlight_depth.is_some_and(|s| s >= item.depth) {
                highlight_depth = None;
            };

            if highlight_depth.is_some() {
                let area = Rect {
                    x: 0,
                    y: line_i as u16,
                    width: buf.area.width,
                    height: 1,
                };

                buf.set_style(
                    area,
                    Style::new().bg(if self.cursor == *item_i {
                        CURRENT_THEME.highlight
                    } else {
                        CURRENT_THEME.dim_highlight
                    }),
                );
            }

            let mut x = 0;
            for span in line.spans.iter() {
                buf.set_stringn(x, line_i as u16, &span.content, span.width(), span.style);
                x += span.width() as u16;
            }
        }
    }
}

fn get_display_text<'a>(item: &crate::items::Item) -> Text<'a> {
    let (ref text, style) = item.display;
    use ansi_to_tui::IntoText;
    let mut text = text.into_text().expect("Couldn't read ansi codes");
    text.patch_style(style);
    text
}
