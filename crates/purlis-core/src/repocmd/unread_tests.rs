//! What brokered git says of the config it does not read (#1413). Every expected answer is
//! written out.

use super::*;

fn entries(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect()
}

const URL: &str = "https://github.com/acme/widget.git";

#[test]
fn a_gitattributes_names_each_filter_it_turns_on_once() {
    let text = "# big files\n*.psd filter=lfs diff=lfs merge=lfs -text\n\
                *.bin filter=lfs\n*.secret filter=crypt\n*.txt -filter\n*.md !filter\n\n";
    assert_eq!(filters_named(text), ["lfs", "crypt"]);
    assert_eq!(filters_named("*.txt text eol=lf\n"), Vec::<String>::new());
}

/// A repository at a new folder whose one commit holds `.gitattributes` as `attributes`, made
/// by `make` (a file or a link), with `README.md` beside it. Made with no global or system git
/// config, so a filter the machine defines (Git LFS's) never rewrites what is committed.
fn committed(make: impl FnOnce(&Path)) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a checkout");
    let top = dir.path();
    assert!(crate::testgit::run_unconfigured(top, &["init", "-q", "-b", "main", "."]).ok());
    std::fs::write(top.join("README.md"), "hello\n").expect("readme");
    make(top);
    assert!(crate::testgit::run_unconfigured(top, &["add", "-A"]).ok());
    assert!(
        crate::testgit::run_unconfigured(
            top,
            &[
                "-c",
                "user.name=T",
                "-c",
                "user.email=t@e.invalid",
                "commit",
                "-q",
                "-m",
                "one"
            ]
        )
        .ok()
    );
    dir
}

#[test]
fn an_lfs_checkout_says_it_holds_pointer_files_and_how_to_fetch_them() {
    let dir = committed(|top| {
        std::fs::write(
            top.join(".gitattributes"),
            "*.psd filter=lfs diff=lfs merge=lfs -text\n*.enc filter=cr`ypt\n",
        )
        .expect("attributes");
    });
    let said = filter_notes(dir.path(), "widget", "workspaces/alpha/widget");
    assert_eq!(said.len(), 2, "{said:?}");
    assert!(said[0].contains("pointer files"), "{said:?}");
    assert!(
        said[0].contains("`git lfs pull` in workspaces/alpha/widget"),
        "{said:?}"
    );
    // A backtick in a name never closes the quote around it.
    assert!(said[1].contains("the `crypt` filter"), "{said:?}");
    // A checkout that names no filter, or is no repository, says nothing.
    let none = committed(|_| {});
    assert_eq!(filter_notes(none.path(), "w", "w"), Vec::<String>::new());
    let bare = tempfile::tempdir().expect("no repository");
    std::fs::write(bare.path().join(".gitattributes"), "* filter=lfs\n").expect("written");
    assert_eq!(filter_notes(bare.path(), "w", "w"), Vec::<String>::new());
}

#[test]
fn what_the_working_tree_says_is_not_read_only_what_head_commits() {
    let dir = committed(|top| {
        std::fs::write(top.join(".gitattributes"), "*.bin filter=lfs\n").expect("attributes");
    });
    // A chat rewrites the file after the checkout: the note still reads the commit.
    std::fs::write(dir.path().join(".gitattributes"), "* filter=chat\n").expect("rewritten");
    let said = filter_notes(dir.path(), "w", "w");
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].contains("Git LFS"), "{said:?}");
}

/// #1550: a filter a folder's own `.gitattributes` turns on is told too, after the top one's,
/// each filter once.
#[test]
fn a_filter_named_only_in_a_folders_gitattributes_is_told() {
    let dir = committed(|top| {
        std::fs::write(top.join(".gitattributes"), "*.psd filter=lfs\n").expect("top");
        std::fs::create_dir_all(top.join("assets/raw")).expect("folders");
        std::fs::write(
            top.join("assets/raw/.gitattributes"),
            "*.bin filter=crypt\n*.psd filter=lfs\n",
        )
        .expect("nested");
        std::fs::write(top.join("assets/raw/one.bin"), "x").expect("a file");
    });
    let said = filter_notes(dir.path(), "w", "w");
    assert_eq!(said.len(), 2, "{said:?}");
    assert!(said[0].contains("Git LFS"), "{said:?}");
    assert!(said[1].contains("the `crypt` filter"), "{said:?}");
    // A repository whose only `.gitattributes` is in a folder is told of it as well.
    let deep = committed(|top| {
        std::fs::create_dir_all(top.join("vendor")).expect("a folder");
        std::fs::write(top.join("vendor/.gitattributes"), "* filter=lfs\n").expect("nested");
    });
    let said = filter_notes(deep.path(), "w", "w");
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].contains("pointer files"), "{said:?}");
}

/// #1550: a folder's `.gitattributes` committed as a link is not followed either, and a name
/// with a pattern's characters in it is read as the name it is.
#[test]
fn a_folders_committed_link_is_never_followed_and_a_name_is_never_a_pattern() {
    let elsewhere = tempfile::tempdir().expect("a file outside");
    let secret = elsewhere.path().join("outside");
    std::fs::write(&secret, "* filter=leaked\n").expect("outside");
    let dir = committed(|top| {
        std::fs::create_dir_all(top.join("linked")).expect("a folder");
        std::os::unix::fs::symlink(&secret, top.join("linked/.gitattributes")).expect("a link");
        std::fs::create_dir_all(top.join("[a]*")).expect("a folder named like a pattern");
        std::fs::write(top.join("[a]*/.gitattributes"), "* filter=odd\n").expect("nested");
    });
    let said = filter_notes(dir.path(), "w", "w");
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].contains("the `odd` filter"), "{said:?}");
}

#[test]
fn a_committed_link_is_never_followed() {
    let elsewhere = tempfile::tempdir().expect("a file outside");
    let secret = elsewhere.path().join("outside");
    std::fs::write(&secret, "* filter=leaked\n").expect("outside");
    let dir = committed(|top| {
        std::os::unix::fs::symlink(&secret, top.join(".gitattributes")).expect("a link");
    });
    assert_eq!(committed_attributes(dir.path()), Vec::<String>::new());
    assert_eq!(filter_notes(dir.path(), "w", "w"), Vec::<String>::new());
}

#[test]
fn a_committed_file_past_the_cap_is_not_read() {
    let dir = committed(|top| {
        let mut text = "* filter=lfs\n".to_owned();
        while (text.len() as u64) <= ATTRIBUTES_AT_MOST {
            text.push_str("# padding padding padding padding padding padding padding\n");
        }
        std::fs::write(top.join(".gitattributes"), text).expect("attributes");
    });
    assert_eq!(committed_attributes(dir.path()), Vec::<String>::new());
    assert_eq!(filter_notes(dir.path(), "w", "w"), Vec::<String>::new());
}

#[test]
fn a_proxy_or_certificate_setting_for_every_url_is_named() {
    let set = entries(&[
        ("http.proxy", "http://proxy.corp:3128"),
        ("http.sslcainfo", "/etc/corp-ca.pem"),
        ("user.name", "Op"),
    ]);
    assert_eq!(route_keys(&set, URL), ["http.proxy", "http.sslCAInfo"]);
}

#[test]
fn a_proxy_switched_off_is_no_route_and_a_client_certificate_is_one() {
    let set = entries(&[
        ("http.proxy", ""),
        ("http.sslcert", "/home/op/client.pem"),
        ("http.https://github.com/.sslkey", "/home/op/client.key"),
        ("http.cookiefile", "/home/op/cookies"),
    ]);
    assert_eq!(
        route_keys(&set, URL),
        ["http.sslCert", "http.<url>.sslKey", "http.cookieFile"]
    );
}

#[test]
fn a_setting_for_one_url_is_named_only_where_it_covers_the_clone() {
    let set = entries(&[
        (
            "http.https://github.com/.extraheader",
            "AUTHORIZATION: basic x",
        ),
        ("http.https://*.corp.example/.proxy", "http://p:1"),
        ("http.https://github.com/other/.sslverify", "false"),
    ]);
    assert_eq!(route_keys(&set, URL), ["http.<url>.extraHeader"]);
    assert_eq!(
        route_keys(&set, "https://git.corp.example/team/svc.git"),
        ["http.<url>.proxy"]
    );
    assert_eq!(
        route_keys(&set, "https://github.com/other/repo.git"),
        ["http.<url>.extraHeader", "http.<url>.sslVerify"]
    );
}

#[test]
fn an_insteadof_rewrite_is_named_when_it_would_rewrite_the_clone_and_never_its_base() {
    let set = entries(&[
        (
            "url.https://mirror.corp/token/github/.insteadof",
            "https://github.com/",
        ),
        ("url.https://elsewhere/.insteadof", "https://gitlab.com/"),
    ]);
    assert_eq!(route_keys(&set, URL), ["url.<base>.insteadOf"]);
    let said = network_note(&set, URL).expect("a note");
    assert!(said.contains("`url.<base>.insteadOf`"), "{said}");
    assert!(
        !said.contains("token"),
        "a value or a base was repeated: {said}"
    );
    assert!(said.contains("in your terminal"), "{said}");
}

#[test]
fn a_config_that_would_not_have_changed_the_route_adds_nothing() {
    let set = entries(&[
        ("user.email", "op@e.invalid"),
        ("http.https://gitlab.com/.proxy", "http://p:1"),
        ("core.editor", "vim"),
    ]);
    assert_eq!(network_note(&set, URL), None);
    assert_eq!(network_note(&[], URL), None);
}

/// #1550: only a clone that could not reach its host, or was refused there, names a key: one
/// that failed for another reason says nothing of the person's config.
#[test]
fn only_a_failure_to_reach_the_host_reads_as_one_a_key_could_have_changed() {
    for reached_not in [
        "fatal: unable to access 'https://github.com/acme/widget.git/': Could not resolve host: \
         github.com",
        "fatal: unable to access 'https://github.com/acme/widget.git/': SSL certificate problem: \
         unable to get local issuer certificate",
        "fatal: unable to access 'https://github.com/acme/widget.git/': Failed to connect to \
         github.com port 443 after 3 ms: Couldn't connect to server",
        "fatal: Authentication failed for 'https://github.com/acme/widget.git/'",
        "fatal: unable to access 'https://github.com/acme/widget.git/': Received HTTP code 407 \
         from proxy after CONNECT",
    ] {
        assert!(reads_as_a_route_failure(reached_not), "{reached_not}");
    }
    for failed_there in [
        "remote: Repository not found.\nfatal: repository \
         'https://github.com/acme/nothing.git/' not found",
        "warning: Could not find remote branch main to clone.\nfatal: Remote branch main not \
         found in upstream origin",
        "fatal: could not create work tree dir 'widget': No space left on device",
        "remote: Repository not found.\nfatal: repository \
         'https://github.com/openssl/proxy-certificate.git/' not found",
        "",
    ] {
        assert!(!reads_as_a_route_failure(failed_there), "{failed_there}");
    }
}
