use std::{io, path::Path, sync::Mutex};

use block2::RcBlock;
use futures::channel::oneshot;
use objc2::rc::Retained;
use objc2_app_kit::{NSRunningApplication, NSWorkspace, NSWorkspaceOpenConfiguration};
use objc2_foundation::{NSArray, NSError, NSString, NSURL};

pub(crate) fn file_url(path: &Path) -> io::Result<Retained<NSURL>> {
    NSURL::from_file_path(std::path::absolute(path)?)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid file URL"))
}

pub(crate) fn open_in_application(
    path: &Path,
    bundle: &str,
) -> io::Result<oneshot::Receiver<io::Result<()>>> {
    let (sender, receiver) = oneshot::channel();
    let sender = Mutex::new(Some(sender));
    open_with_completion(path, bundle, move |result| {
        let sender = match sender.lock() {
            Ok(mut sender) => sender.take(),
            Err(poisoned) => poisoned.into_inner().take(),
        };
        if let Some(sender) = sender {
            drop(sender.send(result));
        }
    })?;
    Ok(receiver)
}

pub(crate) fn open_in_application_detached(path: &Path, bundle: &str) -> io::Result<()> {
    open_with_completion(path, bundle, |result| {
        if let Err(error) = result {
            eprintln!("Native application handoff failed: {error}");
        }
    })
}

fn open_with_completion(
    path: &Path,
    bundle: &str,
    completed: impl Fn(io::Result<()>) + Send + Sync + 'static,
) -> io::Result<()> {
    let access = crate::file_access::read(path)?;
    let url = file_url(path)?;
    let workspace = NSWorkspace::sharedWorkspace();
    let application = workspace
        .URLForApplicationWithBundleIdentifier(&NSString::from_str(bundle))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("Application not found: {bundle}")))?;
    let configuration = NSWorkspaceOpenConfiguration::configuration();
    configuration.setPromptsUserIfNeeded(true);
    configuration.setActivates(true);
    let completion = RcBlock::new(move |application: *mut NSRunningApplication, error: *mut NSError| {
        let _access = &access;
        let result = match unsafe { error.as_ref() } {
            Some(error) => Err(io::Error::other(format!(
                "{} ({}, {})", error.localizedDescription(), error.domain(), error.code()
            ))),
            None if application.is_null() => Err(io::Error::other("macOS did not return an opened application")),
            None => Ok(()),
        };
        completed(result);
    });
    workspace.openURLs_withApplicationAtURL_configuration_completionHandler(
        &NSArray::from_slice(&[&*url]),
        &application,
        &configuration,
        Some(&completion),
    );
    Ok(())
}

pub(crate) fn open_default(path: &Path) -> io::Result<()> {
    let _access = crate::file_access::read(path)?;
    let url = file_url(path)?;
    if NSWorkspace::sharedWorkspace().openURL(&url) {
        Ok(())
    } else {
        Err(io::Error::other("macOS could not open the file in its default application"))
    }
}

pub(crate) fn reveal(path: &Path) -> io::Result<()> {
    let _access = crate::file_access::read(path)?;
    let url = file_url(path)?;
    NSWorkspace::sharedWorkspace().activateFileViewerSelectingURLs(&NSArray::from_slice(&[&*url]));
    Ok(())
}
