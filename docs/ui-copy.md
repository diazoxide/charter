# How the window talks

**Plain, active and specific.** Every label, button, empty state and error in purlis says what
is true, in words the reader already has, and says what to do next when there is something to
do. This page writes down how the window already talks, so new copy matches it. Every example
below is a string the app ships today.

`docs/design-system.md` covers how the window looks. ADR 0072 covers which nouns it may use. This
page covers how it puts them into sentences. ST9's definition of done asks that every feature's
empty state and error copy follow it.

## The voice

- **purlis is lowercase**, at the start of a sentence too, and in the About dialog's title
  (V93a). It is the program's name, and it does the acting: *purlis could not read the alerts*,
  *purlis checks on its own every few hours*, *This is purlis {version}*, *About purlis*. A
  capital Charter or Purlis in the window's text is a fault the copy guard names.
- **The reader is "you".** *You have not opened a project yet.* *Nothing is written into your
  repo.*
- **Active and present.** Say who does what: *It checks again when you press Delete*, not
  *a check will be performed*.
- **Say what is true, then stop.** One claim per sentence. Don't use "simply", "just", "please"
  or "successfully". A save that worked says *Session saved*, not *Session saved successfully!*
- **No exclamation marks**, and no emoji.
- **Plain words for the result, not the mechanism.** *Its value is gone from the vault for good.*
  If the reader needs a mechanism to decide, name it in one clause: *It needs sudo, so purlis
  types it in a shell tab and leaves running it to you.*

## Case

- **Sentence case everywhere:** buttons, menu items, headings, tab titles, dialog titles,
  placeholders and `aria-label`s. Write *Create workspace*, *Open project…* and *Recent
  projects*, never *Open Project…*.
- **Names keep their own capitals:** GitHub, GitLab, Claude Code, Codex, opencode, Keychain,
  Touch ID.
- **A control named inside a sentence is written as its label reads**, with no quotes: *start
  one and press Check again*, *Make one with the + above, or New vault… in the palette*. The
  capital tells the reader it is something to press. An act named for its button is cased
  that way wherever it is the subject, too: *Smart close stopped — …* (#1156).

## Buttons and menu items

- **A verb and what it acts on:** *Create branch*, *Delete vault*, *Restart now*, *Open in its
  pane*. A button that confirms names the act it confirms. It is never *OK* or *Yes*.
- **Cancel is the way back** and changes nothing. A second way out says how it differs: *Wait*
  beside *Restart now*, *Keep it off* beside *Turn the sandbox on*.
- **An ellipsis means "asks something first".** *New project…*, *Browse…*, *Add an extension…*
  open a dialog or a picker before anything happens. A button that acts at once has none.
  Always the one character `…`, never three dots.
- **The same act has the same words everywhere:** the palette row, the menu item, the button
  and the empty state's way out. The window's catalogue (`actions.ts`) is where each act's
  label is written once.

## Work in progress

- **A present participle and an ellipsis:** *Reading the workspace…*, *Asking git…*, *Copying
  your repo into its workspace…*. Say what is being read or asked, not just *Loading…*.
- Something that is not done yet is *Not read yet.* or *not written yet*, not blank.

## Empty states

`EmptyState.tsx` draws them, and its fields are the rule:

- **The headline is a claim about what is true, never an instruction.** *No workspaces yet*,
  *No chats in this workspace*, *Nothing needs you here.*, *Nothing remembered yet*. Use "yet"
  when the thing is expected to arrive.
- **The body is the way out, named the way the window names it:** *Make one with the + above,
  or New persona… in the palette.*, *Add one in the box above.*, *purlis runs each chat in its own pane. Open
  the first one here.*
- **Never the storage underneath.** Leave out file layouts, store names and ADR numbers. *Todos
  are files in this workspace's store* told the reader nothing they could act on.
- **A panel's way out is a catalogue row.** A panel written in Rust names it in
  `panel::Empty::offer`, and the window draws it under the body as a button with the row's own
  title, only while the catalogue offers it. A command in the body goes in backticks, and the
  window draws it in code font.
- **An empty state with no way out is fine.** When there is nothing the reader can do, say so
  and stop. Don't invent an action.
- **Something that has gone says it has gone:** *This memory is not here any more*, *Nothing
  is at {path} now.*

## Errors and refusals

- **Say what happened, from purlis's side, then why:** *purlis could not list the branches of
  {repo}: {reason}*. The reason after the colon is the underlying error, passed through as it
  was said.
- **Then what to do, when there is something to do:** *Could not clone {repos} into {workspace}
  — {reason} Retry from the workspace's settings.* A message with nothing to do ends after the
  reason.
- **Never a stock phrase.** *Something went wrong*, *An error occurred*, *Unknown error*,
  *Oops* and a leading *Error:* each say that nothing was found out. If purlis really does not
  know, say what it was doing: *purlis did not answer with a reading*.
- **A refusal says what purlis will not do, and why:** *purlis cannot send {key}.*, *purlis
  will not read …*. If it does something else instead, say that too: *{name} did not start
  ({reason}). It is still recorded, and will be tried again at the next launch.*
  (`PlaneView.tsx`, the notice for a chat that did not reopen).
- **Don't blame the reader.** The subject is what failed, not what the reader did wrong.
- **Say the cost of an act that cannot be undone, before it happens:** *There is no undo.*,
  *Its 2 secrets are destroyed in your system keychain and cannot be recovered.*
- **Uncertainty is stated, not hidden:** *{name} reports no state, so purlis cannot tell
  whether it is mid-turn.*

## Words

- **The nouns are ADR 0072's.** Five concepts (Project, Workspace, Chat, Persona, Memory), and
  on the first-hour surfaces only those plus *Save* and *branch*. `CONTEXT.md` defines each
  word and lists the ones to avoid. A code repository is always a **repo**, on every forge,
  never a project.
- **No jargon in the window:** no ADR or ticket numbers, no internal type, module or store
  names, no protocol names where the effect can be said instead.
- **A command is written in code font** (`<code>`) where the command line is the way to do
  something: *You can add it later with <code>purlis workspace vision</code>.* Only name
  commands the `purlis` binary has. A panel's text from Rust carries a command in backticks,
  and the window draws it in code font (#1156).
- **Counts are digits, and plurals agree:** *1 secret*, *2 secrets* (`counted` in
  `Vaults.tsx`).
- **An `aria-label` says what the visible label says**, and an icon-only control's label is the
  verb it would have had as text: *Close find*, *Next match*.

## What the build checks

`app/src/copy.test.ts` reads every string in `app/src` (`uiStrings.ts` finds them with
TypeScript's parser) and fails on:

- **a stock phrase**, in any string: *something went wrong*, *an error occurred*, *unknown
  error*, *oops*, *please*, *successfully*, or a leading *error:*. A code span and an id or a
  path (`ask.please`, `hooks/please-hold.sh`) are not read as words;
- **an exclamation mark** ending any sentence of text the window shows;
- **a word in capitals** in text the window shows: four letters or more, all capitals, as in
  *NEVER close*. Acronyms and the words the window keeps in capitals (`ACRONYMS` in `copy.ts`:
  *JSON*, *README*, *PATH*, a workspace's *LIVE* and *LOCAL*), names, key chords, code spans
  and file names such as `AGENTS.md` are taken out first;
- **title case** in text the window shows: two or more words of four letters or more, all
  capitalised. Names (`NAMES` in `copy.ts`) and key chords such as `Ctrl+Shift+F` are taken
  out first. A name the check does not know fails on its first label, and adding it to
  `NAMES` is the fix;
- **a capital "purlis"** in text the window shows, anywhere but the About dialog's title.

"Text the window shows" is:

- JSX text, and a JSX child in braces;
- the value of an attribute a person reads or hears: `aria-label`, `title`, `placeholder`,
  `alt`, `label`, and `EmptyState`'s `headline` and `body`;
- a `label:` property;
- the catalogue's titles and reasons: the second argument of `can` and `cannot` in
  `actions.ts`, and the third of `cannot`.

Through an expression, both branches of a conditional and the right-hand side of `&&`, `||` and
`??` count as shown; the condition does not.

Copy written in Rust and sent to the window is held to the same rules by `copy.rust.test.ts`,
which reads the string literals of named places in the Rust source and builds nothing:

- purlis's own panels' empty states (a headline and a body, not the offer's id);
- the native menu's and the tray's labels;
- the core's `in_window` sentences, and what an `in_window` body calls in its own file: a
  helper function, a `const`, and `self.to_string()`, read as the type's `Display` and its
  variants' `#[error(…)]` text. A call into another module is either read or named in
  `NOT_FOLLOWED` with why it holds no copy, or the check fails;
- the errors of every `#[tauri::command]` in `app/src-tauri/src`: the literals inside its
  `Err(…)`, `map_err(…)`, `ok_or(…)` and `ok_or_else(…)`. The window shows a command's error
  word for word. A file is found by its attribute, so a new command is read without a list.

A format string's `{name}` reads as `…`. A new place the window shows Rust copy from is added to
its `PLACES`. A fault found there that cannot be fixed at once goes in its `KEPT`, with why, and
on #1156's checklist.

Outside the check: copy assembled from parts at run time, and an error the core writes for the
terminal that a command passes on as it was said (`map_err(|err| err.to_string())`). That text
reaches the window, but the source cannot tell it from terminal-only `Display` text. Such an
error gets an `in_window` of its own when the window needs its words, as `worktree::Refusal`
has. DS-8's audit of every surface reads what the check cannot, and a review reads every new
string against this page.
