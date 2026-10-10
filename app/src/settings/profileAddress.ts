/**
 * **Where a harness profile is in Settings** (ST-4, #1296), apart from the Project level's
 * builder so a way in (the new-chat picker's refusals) can name it without building the level.
 *
 * A profile's own page is a sub-page of Harness & profiles, and its address says so: it starts
 * with `project.harness.`, so a link to a page that is not there (a committed profile, which has
 * none, or one renamed away since the link was made) lands on Harness & profiles, the longest
 * group its address starts with (`SettingsTab`'s landing rule).
 */

/** The address of the profile `name`'s own page: a sub-page of Harness & profiles. */
export function profilePage(name: string): string {
  return `project.harness.profile.${name}`;
}

/**
 * The address of the profile `name`'s command row, on its own page: the row a refusal about the
 * profile is mended in. Built as `driver.ts`'s `fileSetting` builds a row's id, from the page,
 * the file (`charter.local.toml`) and the key `[harness.<name>] command`.
 */
export function profileCommand(name: string): string {
  return `${profilePage(name)}.local.harness.${name}.command`;
}
