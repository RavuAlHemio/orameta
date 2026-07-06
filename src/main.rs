use std::collections::BTreeMap;
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
ORDER BY    owner
        ,   table_name
";

const VIEWS_QUERY: &str = "
SELECT  owner
    ,   view_name
    ,   text
FROM    sys.all_views
WHERE   owner = :1
ORDER BY    owner
        ,   view_name
";

const COLUMNS_QUERY: &str = "
SELECT  column_name
    ,   data_type
    ,   data_precision
    ,   data_scale
    ,   nullable
    ,   data_default
    ,   char_length
    ,   char_used
    ,   identity_column
    ,   collation
FROM    sys.all_tab_columns
WHERE   owner = :1
AND     table_name = :2
ORDER BY    column_id
";

const CONSTRAINTS_QUERY: &str = "
SELECT  constraint_name
    ,   constraint_type
    ,   search_condition
    ,   r_owner
    ,   r_constraint_name
    ,   delete_rule
    ,   deferrable
    ,   deferred
FROM    sys.all_constraints
WHERE   owner = :1
AND     table_name = :2
ORDER BY    CASE constraint_type
                WHEN 'P' THEN 1
                WHEN 'U' THEN 2
                WHEN 'R' THEN 3
                WHEN 'C' THEN 4
                ELSE 5
            END
        ,   constraint_type
        ,   constraint_name
";

const CONSTRAINT_COLUMNS_QUERY: &str = "
SELECT  column_name
FROM    sys.all_cons_columns
WHERE   owner = :1
AND     table_name = :2
AND     constraint_name = :3
ORDER BY    position
";

const INDEX_QUERY: &str = "
SELECT  owner
    ,   index_name
    ,   index_type
    ,   uniqueness
FROM    sys.all_indexes
WHERE   table_owner = :1
AND     table_name = :2
ORDER BY    owner
        ,   index_name
";

const INDEX_COLUMNS_QUERY: &str = "
SELECT  column_name
    ,   column_position
    ,   descend
FROM    sys.all_ind_columns
WHERE   table_owner = :1
AND     table_name = :2
AND     index_owner = :3
AND     index_name = :4
ORDER BY    column_position
";

const INDEX_EXPRESSIONS_QUERY: &str = "
SELECT  column_expression
    ,   column_position
FROM    sys.all_ind_expressions
WHERE   table_owner = :1
AND     table_name = :2
AND     index_owner = :3
AND     index_name = :4
ORDER BY    column_position
";


struct StatementContainer {
    pub tables_stmt: Statement,
    pub views_stmt: Statement,
    pub columns_stmt: Statement,
    pub constraints_stmt: Statement,
    pub constraint_columns_stmt: Statement,
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
    pub data_precision: Option<i64>,
    pub data_scale: Option<i64>,
    pub nullable: bool,
    pub default: Option<String>,
    pub string_length: i64,
    pub string_length_type: StringLengthType,
    pub identity_column: bool,
    pub collation: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
enum StringLengthType {
    #[default] NotAString,
    Bytes,
    Characters,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct Constraint {
    pub name: String,
    pub kind: ConstraintKind,
    pub delete_rule: Option<String>,
    pub deferrable: bool,
    pub deferred: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
enum ConstraintKind {
    Primary {
        columns: Vec<String>,
    },
    Unique {
        columns: Vec<String>,
    },
    Foreign {
        columns: Vec<String>,
        ref_schema: String,
        ref_constraint: String,
    },
    Check {
        expression: String,
    },
}
impl ConstraintKind {
    pub fn columns(&self) -> Option<&Vec<String>> {
        match self {
            Self::Primary { columns } => Some(columns),
            Self::Unique { columns } => Some(columns),
            Self::Foreign { columns, .. } => Some(columns),
            Self::Check { .. } => None,
        }
    }

    pub fn columns_mut(&mut self) -> Option<&mut Vec<String>> {
        match self {
            Self::Primary { columns } => Some(columns),
            Self::Unique { columns } => Some(columns),
            Self::Foreign { columns, .. } => Some(columns),
            Self::Check { .. } => None,
        }
    }
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
struct ConstraintTarget {
    pub table_name: String,
    pub columns: Vec<String>,
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
    let constraint_columns_stmt = conn.statement(CONSTRAINT_COLUMNS_QUERY)
        .build().expect("failed to build list-constraint-columns statement");
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
        constraint_columns_stmt,
        index_stmt,
        index_columns_stmt,
        index_expressions_stmt,
    };
    for schema in &config.schemas {
        dump_schema(&mut statement_container, schema);
    }
}

fn print_col_list<T: AsRef<str>>(columns: &[T]) {
    print!("(");
    let mut first_col = true;
    for col in columns {
        if first_col {
            first_col = false;
        } else {
            print!(", ");
        }
        print!("{}", QuoteIdentifier(col.as_ref()));
    }
    print!(")");
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

    for table in &mut tables {
        let columns_result = statement_container.columns_stmt.query(&[&schema, &table.name])
            .expect("failed to query columns");
        for row_res in columns_result {
            let row = row_res
                .expect("failed to obtain table row");

            let name: String = row.get(0).unwrap();
            let data_type: String = row.get(1).unwrap();
            let data_precision: Option<i64> = row.get(2).unwrap();
            let data_scale: Option<i64> = row.get(3).unwrap();
            let nullable_string: String = row.get(4).unwrap();
            let default: Option<String> = row.get(5).unwrap();
            let string_length: i64 = row.get(6).unwrap();
            let string_length_type_string: Option<String> = row.get(7).unwrap();
            let identity_column_string: String = row.get(8).unwrap();
            let collation: Option<String> = row.get(9).unwrap();

            let nullable = match nullable_string.as_ref() {
                "Y" => true,
                "N" => false,
                other => panic!("invalid nullability string: {:?}", other),
            };
            let identity_column = match identity_column_string.as_ref() {
                "YES" => true,
                "NO" => false,
                other => panic!("invalid identity column string: {:?}", other),
            };
            let string_length_type = match string_length_type_string.as_ref().map(|cu| cu.as_str()) {
                Some("B") => StringLengthType::Bytes,
                Some("C") => StringLengthType::Characters,
                None => StringLengthType::NotAString,
                other => panic!("invalid string length type: {:?}", other),
            };

            table.columns.push(Column {
                name,
                data_type,
                data_precision,
                data_scale,
                nullable,
                default,
                string_length,
                string_length_type,
                identity_column,
                collation,
            });
        }

        let constraints_result = statement_container.constraints_stmt.query(&[&schema, &table.name])
            .expect("failed to query constraints");
        for row_res in constraints_result {
            let row = row_res
                .expect("failed to obtain table row");

            let name: String = row.get(0).unwrap();
            let kind: String = row.get(1).unwrap();
            let check_condition: Option<String> = row.get(2).unwrap();
            let ref_schema: Option<String> = row.get(3).unwrap();
            let ref_constraint: Option<String> = row.get(4).unwrap();
            let delete_rule: Option<String> = row.get(5).unwrap();
            let deferrable_string: String = row.get(6).unwrap();
            let deferred_string: String = row.get(7).unwrap();

            let constraint_kind = match &*kind {
                "P" => ConstraintKind::Primary {
                    columns: Vec::new(),
                },
                "U" => ConstraintKind::Unique {
                    columns: Vec::new(),
                },
                "R" => ConstraintKind::Foreign {
                    // "Referential integrity"
                    columns: Vec::new(),
                    ref_schema: ref_schema.unwrap(),
                    ref_constraint: ref_constraint.unwrap(),
                },
                "C" => ConstraintKind::Check {
                    expression: check_condition.unwrap(),
                },
                other => panic!("unknown constraint type {:?}", other),
            };

            let deferrable = match &*deferrable_string {
                "DEFERRABLE" => true,
                "NOT DEFERRABLE" => false,
                other => panic!("unknown deferability {:?}", other),
            };
            let deferred = match &*deferred_string {
                "DEFERRED" => true,
                "IMMEDIATE" => false,
                other => panic!("unknown initial defer state {:?}", other),
            };

            table.constraints.push(Constraint {
                name,
                kind: constraint_kind,
                delete_rule,
                deferrable,
                deferred,
            });
        }

        for constraint in &mut table.constraints {
            if let Some(columns) = constraint.kind.columns_mut() {
                let concols_result = statement_container.constraint_columns_stmt
                    .query(&[&schema, &table.name, &constraint.name])
                    .expect("failed to query constraint columns");
                for row_res in concols_result {
                    let row = row_res
                        .expect("failed to obtain table row");
                    let name: String = row.get(0).unwrap();
                    columns.push(name);
                }
            }
        }

        let indexes_result = statement_container.index_stmt.query(&[&schema, &table.name.as_str()])
            .expect("failed to query indexes");
        for row_res in indexes_result {
            let row = row_res
                .expect("failed to obtain table row");

            let index_schema: String = row.get(0).unwrap();
            let name: String = row.get(1).unwrap();
            let kind: String = row.get(2).unwrap();
            let uniqueness_string: String = row.get(3).unwrap();

            let unique = match &*uniqueness_string {
                "UNIQUE" => true,
                "NONUNIQUE" => false,
                other => panic!("unknown uniqueness value {:?}", other),
            };

            table.indexes.push(Index {
                schema: index_schema,
                name,
                kind,
                unique,
                fields: Vec::new(),
            });
        }

        for index in &mut table.indexes {
            let mut position_to_field = BTreeMap::new();

            let columns_result = statement_container.index_columns_stmt
                .query(&[&schema, &table.name.as_str(), &index.schema.as_str(), &index.name.as_str()])
                .expect("failed to query index columns");
            for row_res in columns_result {
                let row = row_res
                    .expect("failed to obtain table row");
                let name: String = row.get(0).unwrap();
                let position: i64 = row.get(1).unwrap();
                let descend_string: String = row.get(2).unwrap();

                let descending = match &*descend_string {
                    "ASC" => false,
                    "DESC" => true,
                    other => panic!("unknown sort value {:?}", other),
                };

                position_to_field.insert(
                    position,
                    IndexField::Column(IndexColumn {
                        position,
                        name,
                        descending,
                    }),
                );
            }

            let expressions_result = statement_container.index_expressions_stmt
                .query(&[&schema, &table.name.as_str(), &index.schema.as_str(), &index.name.as_str()])
                .expect("failed to query index expressions");
            for row_res in expressions_result {
                let row = row_res
                    .expect("failed to obtain table row");
                let expression: String = row.get(0).unwrap();
                let position: i64 = row.get(1).unwrap();
                position_to_field.insert(
                    position,
                    IndexField::Expression(IndexExpression {
                        position,
                        expression,
                    }),
                );
            }

            for (_position, field) in position_to_field {
                index.fields.push(field);
            }
        }
    }

    // collect the columns of all constraints (to resolve foreign keys)
    let mut constraint_to_target: BTreeMap<String, ConstraintTarget> = BTreeMap::new();
    for table in &tables {
        for constraint in &table.constraints {
            if let Some(columns) = constraint.kind.columns() {
                let target = ConstraintTarget {
                    table_name: table.name.clone(),
                    columns: columns.clone(),
                };
                constraint_to_target.insert(
                    constraint.name.clone(),
                    target,
                );
            }
        }
    }

    for table in &tables {
        println!("CREATE TABLE {}.{} (", QuoteIdentifier(&table.schema), QuoteIdentifier(&table.name));
        let mut first_entry = true;
        for column in &table.columns {
            if first_entry {
                first_entry = false;
                print!("  ");
            } else {
                print!(", ");
            }

            // "NAME" VARCHAR
            print!("{} {}", QuoteIdentifier(&column.name), column.data_type);
            if let Some(precision) = column.data_precision {
                // (1, 2)
                print!("({}", precision);
                if let Some(scale) = column.data_scale {
                    print!(", {}", scale);
                }
                print!(")");
            } else if column.string_length > 0 {
                // (256 CHAR)
                print!(
                    "({} {})",
                    column.string_length,
                    match column.string_length_type {
                        StringLengthType::Bytes => "BYTE",
                        StringLengthType::Characters => "CHAR",
                        _ => panic!("non-char column {:?} typed {:?} with string length", column.name, column.data_type),
                    },
                );
            }

            // NOT NULL
            if !column.nullable {
                print!(" NOT NULL");
            }

            if let Some(default) = column.default.as_ref() {
                print!(" DEFAULT {}", default);
            }

            println!();
        }

        for constraint in &table.constraints {
            if first_entry {
                first_entry = false;
                print!("  ");
            } else {
                print!(", ");
            }

            print!("CONSTRAINT {}", QuoteIdentifier(&constraint.name));
            match &constraint.kind {
                ConstraintKind::Primary { columns } => {
                    print!(" PRIMARY KEY ");
                    print_col_list(columns);
                },
                ConstraintKind::Unique { columns } => {
                    print!(" UNIQUE ");
                    print_col_list(columns);
                },
                ConstraintKind::Foreign { columns, ref_schema, ref_constraint } => {
                    print!(" FOREIGN KEY ");
                    print_col_list(columns);

                    // can we find the referenced constraint?
                    match constraint_to_target.get(ref_constraint) {
                        Some(target) => {
                            // yes; good
                            print!(" REFERENCES {}.{}", QuoteIdentifier(ref_schema), QuoteIdentifier(&target.table_name));
                            print_col_list(&target.columns);
                        },
                        None => {
                            // no; cop out using fake syntax
                            print!(" REFERENCES CONSTRAINT {}.{}", QuoteIdentifier(ref_schema), QuoteIdentifier(ref_constraint));
                        },
                    }
                },
                ConstraintKind::Check { expression } => {
                    print!(" CHECK ({})", expression);
                },
            }

            println!();
        }

        println!(");");

        for index in &table.indexes {
            print!("CREATE");
            if index.unique {
                print!(" UNIQUE");
            }
            print!(
                " INDEX {}.{} ON {}.{} (",
                QuoteIdentifier(&index.schema), QuoteIdentifier(&index.name),
                QuoteIdentifier(&table.schema), QuoteIdentifier(&table.name),
            );
            let mut first_field = true;
            for field in &index.fields {
                if first_field {
                    first_field = false;
                } else {
                    print!(", ");
                }

                match field {
                    IndexField::Column(val) => {
                        print!("{}", QuoteIdentifier(&val.name));
                        if val.descending {
                            print!(" DESC");
                        }
                    },
                    IndexField::Expression(val) => {
                        print!("{}", val.expression);
                    },
                }
            }

            println!(");");
        }

        println!();
    }

    for view in &views {
        println!("CREATE VIEW {}.{} AS", QuoteIdentifier(&view.schema), QuoteIdentifier(&view.name));
        println!("{}", view.definition);
        println!(";");
        println!();
    }
}
