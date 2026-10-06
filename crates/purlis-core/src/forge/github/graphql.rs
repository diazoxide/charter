//! GitHub's GraphQL operations that are not pinned by the recorded Python behaviour: each one a
//! `.graphql` file in `queries/`, checked against `schema.graphql` (GitHub's published schema,
//! V32a) when charter builds, so a field GitHub renamed fails the build and not a user
//! (ADR 0070 §3).
//!
//! Only the documents and the answer types come from here. The request is an ordinary
//! [`crate::forge::transport::Call`], so either transport sends it, and the variables are its
//! fields.

#![allow(clippy::upper_case_acronyms)]

use graphql_client::GraphQLQuery;

// GitHub's custom scalars, as the JSON they travel in. The names are the schema's, which the
// generated code refers to.
type URI = String;
type Date = String;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/forge/github/schema.graphql",
    query_path = "src/forge/github/queries/project.graphql",
    response_derives = "Debug"
)]
pub struct Project;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/forge/github/schema.graphql",
    query_path = "src/forge/github/queries/project_items.graphql",
    response_derives = "Debug"
)]
pub struct ProjectItems;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/forge/github/schema.graphql",
    query_path = "src/forge/github/queries/add_project_item.graphql",
    response_derives = "Debug"
)]
pub struct AddProjectItem;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/forge/github/schema.graphql",
    query_path = "src/forge/github/queries/set_project_field.graphql",
    response_derives = "Debug"
)]
pub struct SetProjectField;

// A commit id, as GitHub's GraphQL spells it.
type GitObjectID = String;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/forge/github/schema.graphql",
    query_path = "src/forge/github/queries/merge_queue.graphql",
    response_derives = "Debug"
)]
pub struct MergeQueue;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/forge/github/schema.graphql",
    query_path = "src/forge/github/queries/enqueue.graphql",
    response_derives = "Debug"
)]
pub struct Enqueue;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/forge/github/schema.graphql",
    query_path = "src/forge/github/queries/work_item.graphql",
    response_derives = "Debug"
)]
pub struct WorkItem;
