### Security

- **A workspace folder gives back only a settings file purlis itself offered there.** When the
  project stops declaring settings, purlis removes the settings it generated in each workspace
  folder. It used to go by a record kept in that folder, which a chat might be able to write.
  Now a file goes only when purlis noted offering that very text at that path, in app state no
  chat can write. When nothing was noted, nothing is removed (#1583).
