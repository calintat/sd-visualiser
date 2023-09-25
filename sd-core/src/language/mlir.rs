#![allow(clippy::clone_on_copy)]

use pest_derive::Parser;

pub struct Mlir;

#[derive(Parser)]
#[grammar = "language/mlir.pest"]
pub struct MlirParser;

mod ast {
    use pest::Span;
    use pest_ast::FromPest;

    use super::Rule;
    use crate::language::span_into_str;

    #[cfg(debug_assertions)]
    #[allow(dead_code, clippy::dbg_macro)]
    fn debug(s: Span) -> Span {
        // useful for debugging pest_ast, use like:
        // #[pest_ast(outer(with(debug), ...))] pub String,
        dbg!(s)
    }

    macro_rules! passthrough {
        (outer, $ast:ident, $rule:ident) => {
            #[derive(Debug, FromPest)]
            #[pest_ast(rule(Rule::$rule))]
            pub struct $ast(
                #[pest_ast(outer(with(span_into_str), with(str::to_string)))] pub String,
            );
        };
        (inner, $ast:ident, $rule:ident) => {
            #[derive(Debug, FromPest)]
            #[pest_ast(rule(Rule::$rule))]
            pub struct $ast(
                #[pest_ast(inner(with(span_into_str), with(str::to_string)))] pub String,
            );
        };
    }

    #[derive(Debug, FromPest)]
    #[pest_ast(rule(Rule::value_use))]
    pub struct Value {
        #[pest_ast(inner(rule(Rule::value_id), with(span_into_str), with(str::to_string)))]
        pub id: String,
        pub index: Option<ValueIndex>,
    }
    #[derive(Debug, FromPest)]
    #[pest_ast(rule(Rule::decimal_literal))]
    pub struct ValueIndex(
        #[pest_ast(outer(with(span_into_str), with(str::parse), with(Result::unwrap)))] pub usize,
    );

    #[derive(Debug, FromPest)]
    #[pest_ast(rule(Rule::attribute_entry))]
    pub struct Attribute(
        #[pest_ast(inner(with(span_into_str), with(str::to_string)))] String,
        #[pest_ast(inner(rule(Rule::attribute_value), with(span_into_str), with(str::to_string)))]
        String,
    );

    #[derive(Debug, FromPest)]
    #[pest_ast(rule(Rule::generic_operation))]
    pub struct GenericOperation {
        #[pest_ast(inner(rule(Rule::string_literal), with(span_into_str), with(str::to_string)))]
        pub op: String,
        pub operands: Vec<Value>,
        pub successors: Vec<Successor>,
        //  data attached to the operation
        pub properties: Vec<Attribute>,
        // enclosed regions
        pub regions: Vec<Region>,
        // attributes are *constant* data, no variables allowed
        pub attributes: Vec<Attribute>,
        #[pest_ast(inner(rule(Rule::function_type), with(span_into_str), with(str::to_string)))]
        pub function_type: String,
    }
    passthrough!(inner, Successor, successor);

    #[derive(Debug, FromPest)]
    #[pest_ast(rule(Rule::op_result))]
    pub struct OpResult {
        #[pest_ast(inner(rule(Rule::value_id), with(span_into_str), with(str::to_string)))]
        pub id: String,
        pub index: Option<OpResultIndex>,
    }
    fn parse_dec_or_hex(s: &str) -> Result<usize, std::num::ParseIntError> {
        if let Some(suffix) = s.strip_prefix("0x") {
            usize::from_str_radix(suffix, 16)
        } else {
            str::parse(s)
        }
    }
    #[derive(Debug, FromPest)]
    #[pest_ast(rule(Rule::integer_literal))]
    pub struct OpResultIndex(
        // can be hex-valued, unlike ValueIndex
        #[pest_ast(outer(with(span_into_str), with(parse_dec_or_hex), with(Result::unwrap)))]
        pub  usize,
    );

    #[derive(Debug, FromPest)]
    #[pest_ast(rule(Rule::operation))]
    pub struct Operation {
        pub result: Vec<OpResult>,
        pub operation: GenericOperation,
        pub location: Option<Location>,
    }
    passthrough!(inner, Location, location);

    #[derive(Debug, FromPest)]
    #[pest_ast(rule(Rule::value_id_and_type))]
    pub struct TypedArg {
        #[pest_ast(inner(rule(Rule::value_id), with(span_into_str), with(str::to_string)))]
        pub id: String,
        #[pest_ast(inner(rule(Rule::r#type), with(span_into_str), with(str::to_string)))]
        pub r#type: String,
    }

    #[derive(Debug, FromPest)]
    #[pest_ast(rule(Rule::block_label))]
    pub struct BlockLabel {
        #[pest_ast(inner(rule(Rule::block_id), with(span_into_str), with(str::to_string)))]
        pub id: String,
        pub args: Vec<TypedArg>,
    }

    #[derive(Debug, FromPest)]
    #[pest_ast(rule(Rule::block))]
    pub struct Block {
        pub label: BlockLabel,
        pub operations: Vec<Operation>,
    }

    #[derive(Debug, FromPest)]
    #[pest_ast(rule(Rule::entry_block))]
    pub struct EntryBlock {
        pub operations: Vec<Operation>,
    }

    #[derive(Debug, FromPest)]
    #[pest_ast(rule(Rule::region))]
    pub struct Region {
        pub entry_block: Option<EntryBlock>,
        pub blocks: Vec<Block>,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::path::Path;

    use dir_test::{dir_test, Fixture};
    use from_pest::FromPest;
    use pest::Parser;

    use super::{MlirParser, Rule};

    #[test]
    fn parse_mlir_operation() -> Result<(), Box<dyn std::error::Error>> {
        let ops = [
            r#"%result:2 = "foo_div"() : () -> (f32, i32)"#,
            r#"%foo, %bar = "foo_div"() : () -> (f32, i32)"#,
            r#"%2 = "tf.scramble"(%result#0, %bar) <{fruit = "banana"}> : (f32, i32) -> f32"#,
            r#"%foo, %bar = "foo_div"() {some_attr = "value", other_attr = 42 : i64} : () -> (f32, i32)"#,
        ];
        for op in ops {
            let mut parse_tree = MlirParser::parse(Rule::operation, op)?;
            let syntax_tree = super::ast::Operation::from_pest(&mut parse_tree)?;
            insta::assert_debug_snapshot!(syntax_tree);
        }
        Ok(())
    }

    #[test]
    fn parse_mlir_block() -> Result<(), Box<dyn std::error::Error>> {
        let blocks = [
            r#"^bb0(%arg0: i64, %arg1: i1):
              "cf.cond_br"(%arg1)[^bb1, ^bb2] {operand_segment_sizes = array<i32: [1, 0, 0]>} : (i1) -> ()
            "#,
            r#"^bb1:  // pred: ^bb0
              "cf.br"(%arg0)[^bb3] : (i64) -> ()
            "#,
            r#"^bb2:  // pred: ^bb0
              %0 = "arith.addi"(%arg0, %arg0) : (i64, i64) -> i64
              "cf.br"(%0)[^bb3] : (i64) -> ()
            "#,
            r#"^bb3(%1: i64):  // 2 preds: ^bb1, ^bb2
              "cf.br"(%1, %arg0)[^bb4] : (i64, i64) -> ()
            "#,
            r#"^bb4(%2: i64, %3: i64):  // pred: ^bb3
              %4 = "arith.addi"(%2, %3) : (i64, i64) -> i64
              "func.return"(%4) : (i64) -> ()
            "#,
        ];
        for block in blocks {
            let mut parse_tree = MlirParser::parse(Rule::block, block)?;
            let syntax_tree = super::ast::Block::from_pest(&mut parse_tree)?;
            insta::assert_debug_snapshot!(syntax_tree);
        }
        Ok(())
    }

    #[test]
    fn parse_mlir_region() -> Result<(), Box<dyn std::error::Error>> {
        let mut parse_tree = MlirParser::parse(
            Rule::region,
            r#"{
                ^bb0(%arg0: i64, %arg1: i1):
                  "cf.cond_br"(%arg1)[^bb1, ^bb2] {operand_segment_sizes = array<i32: [1, 0, 0]>} : (i1) -> ()
                ^bb1:  // pred: ^bb0
                  "cf.br"(%arg0)[^bb3] : (i64) -> ()
                ^bb2:  // pred: ^bb0
                  %0 = "arith.addi"(%arg0, %arg0) : (i64, i64) -> i64
                  "cf.br"(%0)[^bb3] : (i64) -> ()
                ^bb3(%1: i64):  // 2 preds: ^bb1, ^bb2
                  "cf.br"(%1, %arg0)[^bb4] : (i64, i64) -> ()
                ^bb4(%2: i64, %3: i64):  // pred: ^bb3
                  %4 = "arith.addi"(%2, %3) : (i64, i64) -> i64
                  "func.return"(%4) : (i64) -> ()
                }
            "#,
        )?;
        let syntax_tree = super::ast::Region::from_pest(&mut parse_tree)?;
        insta::assert_debug_snapshot!(syntax_tree);
        Ok(())
    }

    type Toplevel = (); // TODO(@NickHu: should have file-based MLIR parse tests)
    pub fn parse_mlir(raw_path: &str) -> (&str, Toplevel) {
        let path = Path::new(raw_path);
        let program = std::fs::read_to_string(path).unwrap();
        let mut pairs = MlirParser::parse(Rule::toplevel, &program).unwrap_or_else(|err| {
            panic!(
                "could not parse program {:?}\n{err:?}",
                path.file_stem().unwrap()
            )
        });
        let name = path.file_stem().unwrap().to_str().unwrap();
        todo!()
    }

    #[ignore]
    #[allow(clippy::needless_pass_by_value)]
    #[dir_test(dir: "$CARGO_MANIFEST_DIR/../examples", glob: "**/*.mlir", loader: crate::language::mlir::tests::parse_mlir, postfix: "check_parse")]
    fn check_parse(fixture: Fixture<(&str, Toplevel)>) {
        let (_name, _expr) = fixture.content();
    }
}
