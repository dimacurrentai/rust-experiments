use pest::Parser;
use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "grammar.pest"]
struct Grammar;

const INPUT_DSL: &str = include_str!("input.dsl");

fn main() {
  match Grammar::parse(Rule::TOPLEVEL, INPUT_DSL) {
    Ok(pairs) => {
      for pair in pairs {
        println!("{:#?}", pair);
      }
    }
    Err(e) => println!("Error: {}", e),
  }
}
