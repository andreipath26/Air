//! Conservative automatic tagging of structured claims, not free-text fact extraction.
use crate::store::{Fact,parse_tsv};
pub fn categories(text:&str)->Vec<&'static str>{
 let words:Vec<String>=text.split(|c:char|!c.is_alphanumeric()).filter(|s|!s.is_empty()).map(str::to_lowercase).collect();
 let rules:[(&str,&[&str]);8]=[("automotive",&["diesel","engine","turbocharger","vehicle","combustion"]),("chemistry",&["oxidation","combustion","molecule","chemical"]),("engineering",&["engine","combustion","mechanical","electrical"]),("geography",&["capital","country","city","continent"]),("programming",&["rust","python","compiler","algorithm"]),("mathematics",&["equation","integer","arithmetic"]),("biology",&["cell","gene","organism"]),("physics",&["energy","force","velocity"])];
 rules.iter().filter(|(_,terms)|terms.iter().any(|t|words.iter().any(|w|w.as_str()==*t))).map(|(c,_)|*c).collect()
}
pub fn curate(line:&str)->std::io::Result<Fact>{let mut f=parse_tsv(line)?;f.categories.retain(|x|!x.trim().is_empty());let text=format!("{} {} {}",f.subject,f.relation,f.object);f.categories.extend(categories(&text).into_iter().map(str::to_owned));if f.categories.is_empty(){f.categories.insert("uncategorised".into());}Ok(f)}
#[test]fn multi_label(){let f=curate("Combustion\tinvolves\toxidation\t\tsource").unwrap();assert!(f.categories.contains("chemistry"));assert!(f.categories.contains("automotive"));assert!(f.categories.contains("engineering"));}
