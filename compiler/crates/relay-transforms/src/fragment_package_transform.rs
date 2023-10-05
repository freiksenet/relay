use std::fs::File;
use std::io::Read;

use common::SourceLocationKey;
use fnv::FnvHashMap;
use fnv::FnvHashSet;
use graphql_ir::Argument;
use graphql_ir::Directive;
use graphql_ir::FragmentDefinition;
use graphql_ir::FragmentDefinitionName;
use graphql_ir::FragmentSpread;
use graphql_ir::OperationDefinition;
use graphql_ir::OperationDefinitionName;
use graphql_ir::Program;
use graphql_ir::TransformedValue;
use graphql_ir::Visitor;
use intern::string_key::StringKey;

pub fn mark_fragment_package(
    program: &Program,
) -> FnvHashMap<FragmentDefinitionName, UsedFragment> {
    let visitor = UsedFragmentVisitor::new();
}

enum UsedFragment {
    Local(StringKey),
    Relative(StringKey, StringKey),
    Absolute(StringKey, StringKey),
}

impl UsedFragment {
    fn as_directive(&self) -> Directive {
      Directive {
        name: "@tmp_internal_fragment_import",
        arguments: vec![
          match self {
          },Argument { name: "fragmentName", value: "" }
        ],
        data: None,
    }
}

struct UsedFragmentVisitor<'s> {
    program: &'s Program,
    location: &'s SourceLocationKey,
    known_fragment_packages: FnvHashMap<SourceLocationKey, StringKey>,
    used_by_operations: FnvHashMap<OperationDefinitionName, FnvHashSet<UsedFragment>>,
    used_by_fragments: FnvHashMap<FragmentDefinitionName, FnvHashSet<UsedFragment>>,
}

impl<'s> UsedFragmentVisitor<'s> {
    pub fn new(program: &'s Program, location: &'s SourceLocationKey) -> Self {
        Self {
            location,
            program,
            known_fragment_packages: Default::default(),
            used_by_operations: Default::default(),
            used_by_fragments: Default::default(),
        }
    }

    pub fn get_package_name(&mut self, location: &SourceLocationKey) -> StringKey {
        *self
            .known_fragment_packages
            .entry(location.clone())
            .or_insert_with(|| {
                self.try_get_package_name(location)
                    .unwrap_or_else(|_| location.path().intern())
            })
    }

    pub fn try_get_package_name(&mut self, location: &SourceLocationKey) -> Result<StringKey, ()> {
        let package_json_dir = find_closest_file("package.json", location.get_dir())?;
        let mut file = File::open(package_json_dir.join("./package.json")).map_err(|_| ())?;
        let mut contents = String::new();
        file.read_to_string(&mut contents).map_err(|_| ())?;
        let serialized_json: serde_json::Value = serde_json::from_str(&contents).map_err(|_| ())?;
        if let serde_json::Value::Object(map) = serialized_json {
            if let Some(serde_json::Value::String(s)) = map.get("name") {
                return Ok(s.intern());
            }
        }
        Err(())
    }
}

impl<'s, 'ir> Visitor for UsedFragmentVisitor<'s> {
    const NAME: &'static str = "UsedFragmentVisitor";
    const VISIT_ARGUMENTS: bool = false;
    const VISIT_DIRECTIVES: bool = false;

    fn visit_operation(&mut self, operation: &OperationDefinition) {
        let location = operation.name.location.source_location();
        let package = self.get_package_name(&location);
        self.default_visit_operation(operation)
    }

    fn visit_fragment(&mut self, fragment: &FragmentDefinition) {
        let location = fragment.name.location.source_location();
        let package = self.get_package_name(&location);
        self.own_location = Some(location);
        self.own_package = Some(package);
        self.default_visit_fragment(fragment)
    }

    fn visit_fragment_spread(&mut self, spread: &FragmentSpread) {
        if self.reachable_fragments.contains_key(&spread.fragment.item) {
            return;
        }

        let fragment = self.program.fragment(spread.fragment.item).unwrap();
        let fragment_name = fragment.name.item;
        let location = &fragment.name.location.source_location();
        let package = self.get_package_name(location);
        let used_fragement = if let (Some(own_location), Some(own_package)) =
            (self.own_location, self.own_package)
        {
            if own_package == package {
                if &own_location == location {
                    UsedFragment::Local(fragment_name.0)
                } else {
                    let dir = RelativePath::from_path(location.get_dir()).unwrap();
                    let own_dir = RelativePath::from_path(own_location.get_dir()).unwrap();
                    UsedFragent::Relative(fragment_name.0, dir.relative_to(own_dir))
                }
            } else {
                UsedFragment::Absolute(fragment_name.0, package)
            }
        } else {
            UsedFragment::Absolute(fragment_name.0, package)
        };
        self.reachable_fragments
            .insert(spread.fragment.item, self.used_fragment);
        fragmen
    }

    fn visit_scalar_field(&mut self, _field: &ScalarField) {
        // Stop
    }
}

fn find_closest_file<P: AsRef<Path>>(filename: &str, current_dir: P) -> Result<PathBuf, String> {
    let mut current_dir = PathBuf::from(current_dir.as_ref());
    loop {
        let file_path = current_dir.join(filename);
        if file_path.exists() {
            return Ok(file_path);
        }
        if !current_dir.pop() {
            return Err(format!(
                "Couldn't find an available \"{}\" from {}.",
                filename,
                current_dir.display()
            ));
        }
    }
}
