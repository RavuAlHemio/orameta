use std::fmt;

use oracle::{Connection, Statement};
use serde::{Deserialize, Serialize};
use toml;


#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct Config {
    username: String,
    password: String,
    connect_string: String,
    schemas: Vec<String>,
}


const TABLES_QUERY: &str = "
SELECT  owner
    ,   table_name
FROM    sys.all_tables
WHERE   owner = :1
";

const VIEWS_QUERY: &str = "
SELECT  owner
    ,   view_name
    ,   text
FROM    sys.all_views
WHERE   owner = :1
";

const COLUMNS_QUERY: &str = "
SELECT  owner
    ,   table_name
    ,   column_name
    ,   data_type
    ,   data_type_mod
    ,   data_length
    ,   data_precision
    ,   nullable
    ,   data_default
    ,   char_length
    ,   char_used
    ,   identity_column
    ,   collation
FROM    sys.all_tab_columns
WHERE   owner = :1
";

const CONSTRAINTS_QUERY: &str = "
SELECT  owner
    ,   table_name
    ,   constraint_name
    ,   constraint_type
    ,   search_condition
    ,   r_owner
    ,   r_constraint_name
    ,   delete_rule
    ,   deferrable
    ,   deferred
    ,   char_length
    ,   char_used
    ,   identity_column
    ,   collation
FROM    sys.all_constraints
WHERE   owner = :1
";

const INDEX_QUERY: &str = "
SELECT  table_owner
    ,   table_name
    ,   owner
    ,   index_name
    ,   index_type
    ,   uniqueness
FROM    sys.all_constraints
WHERE   table_owner = :1
";

const INDEX_COLUMNS_QUERY: &str = "
SELECT  table_owner
    ,   table_name
    ,   index_owner
    ,   index_name
    ,   column_name
    ,   column_position
    ,   descend
FROM    sys.all_ind_columns
WHERE   table_owner = :1
ORDER BY    table_owner
        ,   table_name
        ,   index_owner
        ,   index_name
        ,   column_position
";

const INDEX_EXPRESSIONS_QUERY: &str = "
SELECT  table_owner
    ,   table_name
    ,   index_owner
    ,   index_name
    ,   column_expression
    ,   column_position
FROM    sys.all_ind_expressions
WHERE   table_owner = :1
ORDER BY    table_owner
        ,   table_name
        ,   index_owner
        ,   index_name
        ,   column_position
";


struct StatementContainer {
    pub tables_stmt: Statement,
    pub views_stmt: Statement,
    pub columns_stmt: Statement,
    pub constraints_stmt: Statement,
    pub index_stmt: Statement,
    pub index_columns_stmt: Statement,
    pub index_expressions_stmt: Statement,
}


#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct Table {
    pub schema: String,
    pub name: String,
    pub columns: Vec<Column>,
    pub constraints: Vec<Constraint>,
    pub indexes: Vec<Index>,
}
impl Table {
    pub fn new<S: Into<String>, N: Into<String>>(schema: S, name: N) -> Self {
        Self {
            schema: schema.into(),
            name: name.into(),
            columns: Vec::new(),
            constraints: Vec::new(),
            indexes: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct View {
    pub schema: String,
    pub name: String,
    pub definition: String,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct Column {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
enum Constraint {
    // TODO
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct Index {
    pub schema: String,
    pub name: String,
    pub kind: String,
    pub unique: bool,
    pub fields: Vec<IndexField>,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
enum IndexField {
    Column(IndexColumn),
    Expression(IndexExpression),
}
impl IndexField {
    pub fn position(&self) -> i64 {
        match self {
            Self::Column(val) => val.position,
            Self::Expression(val) => val.position,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct IndexColumn {
    pub position: i64,
    pub name: String,
    pub descending: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct IndexExpression {
    pub position: i64,
    pub expression: String,
}


#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct QuoteIdentifier<'a>(&'a str);
impl<'a> fmt::Display for QuoteIdentifier<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "\"")?;
        for c in self.0.chars() {
            if c == '"' {
                // double
                write!(f, "\"")?;
            }
            write!(f, "{}", c)?;
        }
        write!(f, "\"")
    }
}


fn main() {
    let config_string = std::fs::read_to_string("config.toml")
        .expect("failed to read config.toml");
    let config: Config = toml::from_str(&config_string)
        .expect("failed to parse config.toml");

    let conn = Connection::connect(&config.username, &config.password, &config.connect_string)
        .expect("failed to connect to Oracle");
    let tables_stmt = conn.statement(TABLES_QUERY)
        .build().expect("failed to build list-tables statement");
    let views_stmt = conn.statement(VIEWS_QUERY)
        .build().expect("failed to build list-views statement");
    let columns_stmt = conn.statement(COLUMNS_QUERY)
        .build().expect("failed to build list-columns statement");
    let constraints_stmt = conn.statement(CONSTRAINTS_QUERY)
        .build().expect("failed to build list-constraints statement");
    let index_stmt = conn.statement(INDEX_QUERY)
        .build().expect("failed to build list-indexes statement");
    let index_columns_stmt = conn.statement(INDEX_COLUMNS_QUERY)
        .build().expect("failed to build list-index-columns statement");
    let index_expressions_stmt = conn.statement(INDEX_EXPRESSIONS_QUERY)
        .build().expect("failed to build list-index-expressions statement");

    let mut statement_container = StatementContainer {
        tables_stmt,
        views_stmt,
        columns_stmt,
        constraints_stmt,
        index_stmt,
        index_columns_stmt,
        index_expressions_stmt,
    };
    for schema in &config.schemas {
        dump_schema(&mut statement_container, schema);
    }
}

fn dump_schema(statement_container: &mut StatementContainer, schema: &str) {
    let tables_result = statement_container.tables_stmt.query(&[&schema])
        .expect("failed to query tables");
    let mut tables = Vec::new();
    for row_res in tables_result {
        let row = row_res
            .expect("failed to obtain table row");
        let schema_name: String = row.get(0).unwrap();
        let table_name: String = row.get(1).unwrap();
        tables.push(Table::new(schema_name, table_name));
    }

    let views_result = statement_container.views_stmt.query(&[&schema])
        .expect("failed to query views");
    let mut views = Vec::new();
    for row_res in views_result {
        let row = row_res
            .expect("failed to obtain table row");
        let schema_name = row.get(0).unwrap();
        let view_name: String = row.get(1).unwrap();
        let definition: String = row.get(2).unwrap();
        views.push(View {
            schema: schema_name,
            name: view_name,
            definition,
        });
    }

    for table in &tables {
        println!("{:?}", table);
    }
    for view in &views {
        println!("CREATE VIEW {}.{} AS", QuoteIdentifier(&view.schema), QuoteIdentifier(&view.name));
        println!("{}", view.definition);
        println!(";");
    }
}
