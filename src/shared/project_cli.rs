//! `jocasta project …` (0111).

use std::path::Path;

use rusqlite::Connection;

use crate::shared::errors::{Error, Result};
use crate::shared::projects::{add_project, list_projects, remove_project, sync_catalog};
use crate::shared::projects_file::read_projects_file;
use crate::shared::schema::open_catalog;

const PROJECT_USAGE: &str = "usage: jocasta project list | add <path> | remove <name|id>";

/// `jocasta project …` (0111). Resolves the lines to print; usage (`--help` too) touches nothing on disk.
pub async fn project_command(args: &[String]) -> Result<Vec<String>> {
	let (verb, rest) = match args.split_first() {
		Some((verb, rest)) => (verb.as_str(), rest),
		None => ("", args),
	};
	let known = (verb == "list" && rest.is_empty())
		|| ((verb == "add" || verb == "remove") && rest.len() == 1 && !rest[0].is_empty());
	if !known {
		return Err(Error::user(PROJECT_USAGE));
	}
	let catalog = open_catalog()?;
	match (verb, rest) {
		("list", _) => list(&catalog).await,
		("add", [path]) => add(&catalog, path).await,
		("remove", [name]) => {
			let project = remove_project(&catalog, name, None).await?;
			Ok(vec![format!("removed {} ({}); its paths are excluded in projects.yaml", project.name, project.id)])
		}
		_ => Err(Error::user(PROJECT_USAGE)),
	}
}

async fn list(catalog: &Connection) -> Result<Vec<String>> {
	let file = read_projects_file(None)?;
	sync_catalog(catalog, Some(&file)).await?;
	let lines: Vec<String> = list_projects(catalog, Some(&file))?
		.into_iter()
		.map(|p| format!("{}  {}  {}", p.id, p.name, p.main_path.display()))
		.collect();
	if lines.is_empty() {
		return Ok(vec![
			"No projects yet: add one with jocasta project add <path>, or a watched root in projects.yaml.".to_owned(),
		]);
	}
	Ok(lines)
}

async fn add(catalog: &Connection, path: &str) -> Result<Vec<String>> {
	let file = read_projects_file(None)?;
	sync_catalog(catalog, Some(&file)).await?;
	let known: Vec<String> = list_projects(catalog, Some(&file))?.into_iter().map(|p| p.id).collect();
	let added = add_project(catalog, Path::new(path), None).await?;
	let project = &added.project;
	if known.contains(&project.id) {
		return Ok(vec![format!(
			"{} ({}) is already a project, at {}",
			project.name,
			project.id,
			project.main_path.display()
		)]);
	}
	let main = match &added.main_checkout_of {
		Some(of) => format!(" (the main checkout of {})", of.display()),
		None => String::new(),
	};
	Ok(vec![format!("added {} ({}) at {}{main}", project.name, project.id, project.main_path.display())])
}

#[cfg(test)]
#[path = "project_cli_tests.rs"]
mod tests;
