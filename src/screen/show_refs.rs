use std::{
    collections::{btree_map::Entry, BTreeMap},
    iter,
    rc::Rc,
};

use super::Screen;
use crate::{
    config::{Config, StyleConfigEntry},
    error::Error,
    items::{self, hash, Item, TargetData},
    Res,
};
use git2::{Reference, Repository};
use ratatui::{
    layout::Size,
    text::{Line, Span},
};

pub(crate) fn create(config: Rc<Config>, repo: Rc<Repository>, size: Size) -> Res<Screen> {
    Screen::new(
        Rc::clone(&config),
        size,
        Box::new(move || {
            let style = &config.style;

            Ok(iter::once(Item {
                id: hash("local_branches"),
                display: Line::styled("Branches".to_string(), &style.section_header),
                section: true,
                depth: 0,
                ..Default::default()
            })
            .chain(
                create_reference_items(&repo, Reference::is_branch, &style.branch)?
                    .map(|(_, item)| item),
            )
            .chain(create_remotes_sections(
                &repo,
                &style.section_header,
                &style.remote,
            )?)
            .chain(create_tags_section(
                &repo,
                &style.section_header,
                &style.tag,
            )?)
            .collect())
        }),
    )
}

fn create_remotes_sections<'a>(
    repo: &'a Repository,
    header_style: &'a StyleConfigEntry,
    item_style: &'a StyleConfigEntry,
) -> Res<impl Iterator<Item = Item> + 'a> {
    let all_remotes = create_reference_items(repo, Reference::is_remote, item_style)?;
    let mut remotes = BTreeMap::new();
    for (name, remote) in all_remotes {
        let name =
            String::from_utf8_lossy(&repo.branch_remote_name(&name).map_err(Error::GetRemote)?)
                .to_string();

        match remotes.entry(name) {
            Entry::Vacant(entry) => {
                entry.insert(vec![remote]);
            }
            Entry::Occupied(mut entry) => {
                entry.get_mut().push(remote);
            }
        }
    }

    Ok(remotes.into_iter().flat_map(move |(name, items)| {
        let header = format!("Remote {name}");
        vec![
            items::blank_line(),
            Item {
                id: hash(&name),
                display: Line::styled(header, header_style),
                section: true,
                depth: 0,
                ..Default::default()
            },
        ]
        .into_iter()
        .chain(items)
    }))
}

fn create_tags_section<'a>(
    repo: &'a Repository,
    header_style: &'a StyleConfigEntry,
    item_style: &'a StyleConfigEntry,
) -> Res<impl Iterator<Item = Item> + 'a> {
    let mut tags = create_reference_items(repo, Reference::is_tag, item_style)?;
    Ok(match tags.next() {
        Some((_name, item)) => vec![
            items::blank_line(),
            Item {
                id: hash("tags"),
                display: Line::styled("Tags".to_string(), header_style),
                section: true,
                depth: 0,
                ..Default::default()
            },
            item,
        ],
        None => vec![],
    }
    .into_iter()
    .chain(tags.map(|(_name, item)| item)))
}

fn create_reference_items<'a, F>(
    repo: &'a Repository,
    filter: F,
    style: &'a StyleConfigEntry,
) -> Res<impl Iterator<Item = (String, Item)> + 'a>
where
    F: FnMut(&Reference<'a>) -> bool + 'a,
{
    Ok(repo
        .references()
        .map_err(Error::ListGitReferences)?
        .filter_map(Result::ok)
        .filter(filter)
        .map(move |reference| {
            let name = reference.name().unwrap().to_owned();
            let shorthand = reference.shorthand().unwrap().to_owned();
            let item = Item {
                id: hash(&name),
                display: Line::from(vec![
                    create_prefix(repo, &reference),
                    Span::styled(shorthand.clone(), style),
                ]),
                depth: 1,
                target_data: Some(TargetData::Branch(shorthand)),
                ..Default::default()
            };
            (name, item)
        }))
}

fn create_prefix(repo: &Repository, reference: &Reference) -> Span<'static> {
    let head = repo.head().ok();

    Span::raw(if repo.head_detached().unwrap_or(false) {
        if reference.target() == head.as_ref().and_then(Reference::target) {
            "? "
        } else {
            "  "
        }
    } else if reference.name() == head.as_ref().and_then(Reference::name) {
        "* "
    } else {
        "  "
    })
}
