table "households" {
  schema = schema.main
  column "singleton" {
    null = false
    type = integer
  }
  column "household_id" {
    null = false
    type = text
  }
  column "name" {
    null = false
    type = text
  }
  column "base_currency" {
    null = false
    type = text
  }
  column "as_of" {
    null = false
    type = text
  }
  column "rule_tie_break" {
    null = false
    type = text
  }
  column "last_saved_at" {
    null = false
    type = integer
  }
  column "last_written_by_version" {
    null = false
    type = text
  }
  column "format_lineage" {
    null = false
    type = text
  }
  primary_key {
    columns = [column.singleton]
  }
  index "households_household_id" {
    unique  = true
    columns = [column.household_id]
  }
  check {
    expr = "(singleton = 1)"
  }
  check {
    expr = "(length(base_currency) = 3)"
  }
  check {
    expr = "(length(as_of) = 10)"
  }
  check {
    expr = "(rule_tie_break IN ('oldest-rule', 'newest-version'))"
  }
}
table "people" {
  schema = schema.main
  column "id" {
    null = false
    type = integer
  }
  column "position" {
    null = false
    type = integer
  }
  column "name" {
    null = false
    type = text
  }
  column "role" {
    null = false
    type = text
  }
  column "payload_json" {
    null = false
    type = text
  }
  primary_key {
    columns = [column.id]
  }
  index "people_position" {
    unique  = true
    columns = [column.position]
  }
  check {
    expr = "(id >= 0)"
  }
  check {
    expr = "(position >= 0)"
  }
  check {
    expr = "(role IN ('owner', 'member', 'dependent', 'adviser', 'read_only'))"
  }
  check {
    expr = "(json_valid(payload_json))"
  }
}
table "companies" {
  schema = schema.main
  column "id" {
    null = false
    type = integer
  }
  column "position" {
    null = false
    type = integer
  }
  column "name" {
    null = false
    type = text
  }
  column "jurisdiction" {
    null = false
    type = text
  }
  column "payload_json" {
    null = false
    type = text
  }
  primary_key {
    columns = [column.id]
  }
  index "companies_position" {
    unique  = true
    columns = [column.position]
  }
  check {
    expr = "(id >= 0)"
  }
  check {
    expr = "(position >= 0)"
  }
  check {
    expr = "(json_valid(payload_json))"
  }
}
table "company_owners" {
  schema = schema.main
  column "company_id" {
    null = false
    type = integer
  }
  column "position" {
    null = false
    type = integer
  }
  column "person_id" {
    null = false
    type = integer
  }
  column "basis_points" {
    null = false
    type = integer
  }
  primary_key {
    columns = [column.company_id, column.position]
  }
  foreign_key "0" {
    columns     = [column.person_id]
    ref_columns = [table.people.column.id]
    on_update   = NO_ACTION
    on_delete   = RESTRICT
  }
  foreign_key "1" {
    columns     = [column.company_id]
    ref_columns = [table.companies.column.id]
    on_update   = NO_ACTION
    on_delete   = CASCADE
  }
  index "company_owners_company_id_person_id" {
    unique  = true
    columns = [column.company_id, column.person_id]
  }
  index "idx_company_owners_person" {
    columns = [column.person_id]
  }
  check {
    expr = "(position >= 0)"
  }
  check {
    expr = "(basis_points BETWEEN 0 AND 10000)"
  }
}
table "company_roles" {
  schema = schema.main
  column "company_id" {
    null = false
    type = integer
  }
  column "position" {
    null = false
    type = integer
  }
  column "person_id" {
    null = false
    type = integer
  }
  column "role" {
    null = false
    type = text
  }
  primary_key {
    columns = [column.company_id, column.position]
  }
  foreign_key "0" {
    columns     = [column.person_id]
    ref_columns = [table.people.column.id]
    on_update   = NO_ACTION
    on_delete   = RESTRICT
  }
  foreign_key "1" {
    columns     = [column.company_id]
    ref_columns = [table.companies.column.id]
    on_update   = NO_ACTION
    on_delete   = CASCADE
  }
  index "company_roles_company_id_person_id_role" {
    unique  = true
    columns = [column.company_id, column.person_id, column.role]
  }
  index "idx_company_roles_person" {
    columns = [column.person_id]
  }
  check {
    expr = "(position >= 0)"
  }
}
table "company_employees" {
  schema = schema.main
  column "company_id" {
    null = false
    type = integer
  }
  column "position" {
    null = false
    type = integer
  }
  column "person_id" {
    null = true
    type = integer
  }
  column "name" {
    null = false
    type = text
  }
  column "monthly_gross_minor" {
    null = false
    type = integer
  }
  column "currency" {
    null = false
    type = text
  }
  column "starts_on" {
    null = false
    type = text
  }
  column "ends_on" {
    null = true
    type = text
  }
  primary_key {
    columns = [column.company_id, column.position]
  }
  foreign_key "0" {
    columns     = [column.person_id]
    ref_columns = [table.people.column.id]
    on_update   = NO_ACTION
    on_delete   = RESTRICT
  }
  foreign_key "1" {
    columns     = [column.company_id]
    ref_columns = [table.companies.column.id]
    on_update   = NO_ACTION
    on_delete   = CASCADE
  }
  index "idx_company_employees_person" {
    columns = [column.person_id]
  }
  check {
    expr = "(position >= 0)"
  }
  check {
    expr = "(length(currency) = 3)"
  }
  check {
    expr = "(length(starts_on) = 10)"
  }
  check {
    expr = "(ends_on IS NULL OR length(ends_on) = 10)"
  }
}
table "company_constraints" {
  schema = schema.main
  column "company_id" {
    null = false
    type = integer
  }
  column "position" {
    null = false
    type = integer
  }
  column "kind" {
    null = false
    type = text
  }
  column "value_json" {
    null = false
    type = text
  }
  primary_key {
    columns = [column.company_id, column.position]
  }
  foreign_key "0" {
    columns     = [column.company_id]
    ref_columns = [table.companies.column.id]
    on_update   = NO_ACTION
    on_delete   = CASCADE
  }
  check {
    expr = "(position >= 0)"
  }
  check {
    expr = "(json_valid(value_json))"
  }
}
table "accounts" {
  schema = schema.main
  column "id" {
    null = false
    type = integer
  }
  column "position" {
    null = false
    type = integer
  }
  column "name" {
    null = false
    type = text
  }
  column "institution" {
    null = false
    type = text
  }
  column "kind" {
    null = false
    type = text
  }
  column "holder_kind" {
    null = false
    type = text
  }
  column "holder_company_id" {
    null = true
    type = integer
  }
  column "currency" {
    null = false
    type = text
  }
  column "liquidity_json" {
    null = false
    type = text
  }
  column "minimum_balance_minor" {
    null = true
    type = integer
  }
  column "minimum_balance_currency" {
    null = true
    type = text
  }
  column "transfer_delay_days" {
    null = false
    type = integer
  }
  column "tax_treatment" {
    null = false
    type = text
  }
  column "source_of_truth" {
    null = false
    type = text
  }
  column "last_reconciled" {
    null = true
    type = text
  }
  column "withdrawals_permitted" {
    null = false
    type = integer
  }
  column "include_in_household" {
    null = false
    type = integer
  }
  column "settled_balance_minor" {
    null = false
    type = integer
  }
  column "pending_balance_minor" {
    null = false
    type = integer
  }
  column "payload_json" {
    null = false
    type = text
  }
  primary_key {
    columns = [column.id]
  }
  foreign_key "0" {
    columns     = [column.holder_company_id]
    ref_columns = [table.companies.column.id]
    on_update   = NO_ACTION
    on_delete   = RESTRICT
  }
  index "accounts_position" {
    unique  = true
    columns = [column.position]
  }
  index "idx_accounts_company" {
    columns = [column.holder_company_id]
  }
  index "idx_accounts_kind" {
    columns = [column.kind]
  }
  check {
    expr = "(id >= 0)"
  }
  check {
    expr = "(position >= 0)"
  }
  check {
    expr = "(holder_kind IN ('persons', 'company'))"
  }
  check {
    expr = "(length(currency) = 3)"
  }
  check {
    expr = "(json_valid(liquidity_json))"
  }
  check {
    expr = "(minimum_balance_currency IS NULL OR length(minimum_balance_currency) = 3)"
  }
  check {
    expr = "(transfer_delay_days BETWEEN 0 AND 65535)"
  }
  check {
    expr = "(last_reconciled IS NULL OR length(last_reconciled) = 10)"
  }
  check {
    expr = "(withdrawals_permitted IN (0, 1))"
  }
  check {
    expr = "(include_in_household IN (0, 1))"
  }
  check {
    expr = "(json_valid(payload_json))"
  }
  check {
    expr = "((holder_kind = 'company' AND holder_company_id IS NOT NULL) OR (holder_kind = 'persons' AND holder_company_id IS NULL))"
  }
}
table "account_owners" {
  schema = schema.main
  column "account_id" {
    null = false
    type = integer
  }
  column "position" {
    null = false
    type = integer
  }
  column "person_id" {
    null = false
    type = integer
  }
  column "basis_points" {
    null = false
    type = integer
  }
  primary_key {
    columns = [column.account_id, column.position]
  }
  foreign_key "0" {
    columns     = [column.person_id]
    ref_columns = [table.people.column.id]
    on_update   = NO_ACTION
    on_delete   = RESTRICT
  }
  foreign_key "1" {
    columns     = [column.account_id]
    ref_columns = [table.accounts.column.id]
    on_update   = NO_ACTION
    on_delete   = CASCADE
  }
  index "account_owners_account_id_person_id" {
    unique  = true
    columns = [column.account_id, column.person_id]
  }
  index "idx_account_owners_person" {
    columns = [column.person_id]
  }
  check {
    expr = "(position >= 0)"
  }
  check {
    expr = "(basis_points BETWEEN 0 AND 10000)"
  }
}
table "account_fees" {
  schema = schema.main
  column "account_id" {
    null = false
    type = integer
  }
  column "position" {
    null = false
    type = integer
  }
  column "description" {
    null = false
    type = text
  }
  column "fixed_minor" {
    null = true
    type = integer
  }
  column "fixed_currency" {
    null = true
    type = text
  }
  column "basis_points" {
    null = false
    type = integer
  }
  primary_key {
    columns = [column.account_id, column.position]
  }
  foreign_key "0" {
    columns     = [column.account_id]
    ref_columns = [table.accounts.column.id]
    on_update   = NO_ACTION
    on_delete   = CASCADE
  }
  check {
    expr = "(position >= 0)"
  }
  check {
    expr = "(fixed_currency IS NULL OR length(fixed_currency) = 3)"
  }
  check {
    expr = "(basis_points >= 0)"
  }
}
table "account_fund_categories" {
  schema = schema.main
  column "account_id" {
    null = false
    type = integer
  }
  column "position" {
    null = false
    type = integer
  }
  column "category" {
    null = false
    type = text
  }
  primary_key {
    columns = [column.account_id, column.position]
  }
  foreign_key "0" {
    columns     = [column.account_id]
    ref_columns = [table.accounts.column.id]
    on_update   = NO_ACTION
    on_delete   = CASCADE
  }
  index "account_fund_categories_account_id_category" {
    unique  = true
    columns = [column.account_id, column.category]
  }
  check {
    expr = "(position >= 0)"
  }
}
table "reservations" {
  schema = schema.main
  column "id" {
    null = false
    type = integer
  }
  column "position" {
    null = false
    type = integer
  }
  column "name" {
    null = false
    type = text
  }
  column "account_id" {
    null = false
    type = integer
  }
  column "amount_minor" {
    null = false
    type = integer
  }
  column "currency" {
    null = false
    type = text
  }
  column "coverage_json" {
    null = false
    type = text
  }
  column "nested_in_id" {
    null = true
    type = integer
  }
  column "hardness" {
    null = false
    type = text
  }
  column "purpose" {
    null = false
    type = text
  }
  column "released_on" {
    null = true
    type = text
  }
  column "payload_json" {
    null = false
    type = text
  }
  primary_key {
    columns = [column.id]
  }
  foreign_key "0" {
    columns     = [column.nested_in_id]
    ref_columns = [table.reservations.column.id]
    on_update   = NO_ACTION
    on_delete   = RESTRICT
  }
  foreign_key "1" {
    columns     = [column.account_id]
    ref_columns = [table.accounts.column.id]
    on_update   = NO_ACTION
    on_delete   = RESTRICT
  }
  index "reservations_position" {
    unique  = true
    columns = [column.position]
  }
  index "idx_reservations_account" {
    columns = [column.account_id, column.released_on]
  }
  index "idx_reservations_nested" {
    columns = [column.nested_in_id]
  }
  check {
    expr = "(id >= 0)"
  }
  check {
    expr = "(position >= 0)"
  }
  check {
    expr = "(amount_minor >= 0)"
  }
  check {
    expr = "(length(currency) = 3)"
  }
  check {
    expr = "(json_valid(coverage_json))"
  }
  check {
    expr = "(hardness IN ('hard', 'soft_user_relaxable'))"
  }
  check {
    expr = "(released_on IS NULL OR length(released_on) = 10)"
  }
  check {
    expr = "(json_valid(payload_json))"
  }
}
table "event_series" {
  schema = schema.main
  column "id" {
    null = false
    type = integer
  }
  column "position" {
    null = false
    type = integer
  }
  column "name" {
    null = false
    type = text
  }
  column "direction_json" {
    null = false
    type = text
  }
  column "expected_minor" {
    null = false
    type = integer
  }
  column "currency" {
    null = false
    type = text
  }
  column "recurrence_json" {
    null = false
    type = text
  }
  column "settlement_lag_days" {
    null = false
    type = integer
  }
  column "availability_lag_days" {
    null = false
    type = integer
  }
  column "intraday_order" {
    null = false
    type = integer
  }
  column "account_id" {
    null = false
    type = integer
  }
  column "linked_account_id" {
    null = true
    type = integer
  }
  column "entity_json" {
    null = false
    type = text
  }
  column "certainty" {
    null = false
    type = text
  }
  column "category" {
    null = false
    type = text
  }
  column "tax_treatment" {
    null = false
    type = text
  }
  column "scenario_id" {
    null = true
    type = integer
  }
  column "notes" {
    null = false
    type = text
  }
  column "payload_json" {
    null = false
    type = text
  }
  primary_key {
    columns = [column.id]
  }
  foreign_key "0" {
    columns     = [column.linked_account_id]
    ref_columns = [table.accounts.column.id]
    on_update   = NO_ACTION
    on_delete   = RESTRICT
  }
  foreign_key "1" {
    columns     = [column.account_id]
    ref_columns = [table.accounts.column.id]
    on_update   = NO_ACTION
    on_delete   = RESTRICT
  }
  index "event_series_position" {
    unique  = true
    columns = [column.position]
  }
  index "idx_event_series_account" {
    columns = [column.account_id, column.position]
  }
  index "idx_event_series_linked_account" {
    columns = [column.linked_account_id]
  }
  index "idx_event_series_scenario" {
    columns = [column.scenario_id]
  }
  check {
    expr = "(id >= 0)"
  }
  check {
    expr = "(position >= 0)"
  }
  check {
    expr = "(json_valid(direction_json))"
  }
  check {
    expr = "(expected_minor >= 0)"
  }
  check {
    expr = "(length(currency) = 3)"
  }
  check {
    expr = "(json_valid(recurrence_json))"
  }
  check {
    expr = "(settlement_lag_days BETWEEN 0 AND 65535)"
  }
  check {
    expr = "(availability_lag_days BETWEEN 0 AND 65535)"
  }
  check {
    expr = "(intraday_order BETWEEN 0 AND 65535)"
  }
  check {
    expr = "(json_valid(entity_json))"
  }
  check {
    expr = "(json_valid(payload_json))"
  }
}
table "series_amount_changes" {
  schema = schema.main
  column "series_id" {
    null = false
    type = integer
  }
  column "position" {
    null = false
    type = integer
  }
  column "effective_from" {
    null = false
    type = text
  }
  column "amount_json" {
    null = false
    type = text
  }
  primary_key {
    columns = [column.series_id, column.position]
  }
  foreign_key "0" {
    columns     = [column.series_id]
    ref_columns = [table.event_series.column.id]
    on_update   = NO_ACTION
    on_delete   = CASCADE
  }
  check {
    expr = "(position >= 0)"
  }
  check {
    expr = "(length(effective_from) = 10)"
  }
  check {
    expr = "(json_valid(amount_json))"
  }
}
table "series_exceptions" {
  schema = schema.main
  column "series_id" {
    null = false
    type = integer
  }
  column "position" {
    null = false
    type = integer
  }
  column "original_due" {
    null = false
    type = text
  }
  column "kind_json" {
    null = false
    type = text
  }
  primary_key {
    columns = [column.series_id, column.position]
  }
  foreign_key "0" {
    columns     = [column.series_id]
    ref_columns = [table.event_series.column.id]
    on_update   = NO_ACTION
    on_delete   = CASCADE
  }
  index "series_exceptions_series_id_original_due" {
    unique  = true
    columns = [column.series_id, column.original_due]
  }
  check {
    expr = "(position >= 0)"
  }
  check {
    expr = "(length(original_due) = 10)"
  }
  check {
    expr = "(json_valid(kind_json))"
  }
}
table "assumptions" {
  schema = schema.main
  column "id" {
    null = false
    type = integer
  }
  column "position" {
    null = false
    type = integer
  }
  column "text" {
    null = false
    type = text
  }
  column "certainty" {
    null = false
    type = text
  }
  column "source_json" {
    null = false
    type = text
  }
  column "accepted_on" {
    null = true
    type = text
  }
  column "expires_on" {
    null = true
    type = text
  }
  column "private_to" {
    null = true
    type = integer
  }
  column "payload_json" {
    null = false
    type = text
  }
  primary_key {
    columns = [column.id]
  }
  foreign_key "0" {
    columns     = [column.private_to]
    ref_columns = [table.people.column.id]
    on_update   = NO_ACTION
    on_delete   = RESTRICT
  }
  index "assumptions_position" {
    unique  = true
    columns = [column.position]
  }
  index "idx_assumptions_private_to" {
    columns = [column.private_to]
  }
  check {
    expr = "(id >= 0)"
  }
  check {
    expr = "(position >= 0)"
  }
  check {
    expr = "(json_valid(source_json))"
  }
  check {
    expr = "(accepted_on IS NULL OR length(accepted_on) = 10)"
  }
  check {
    expr = "(expires_on IS NULL OR length(expires_on) = 10)"
  }
  check {
    expr = "(json_valid(payload_json))"
  }
}
table "assumption_series" {
  schema = schema.main
  column "assumption_id" {
    null = false
    type = integer
  }
  column "position" {
    null = false
    type = integer
  }
  column "series_id" {
    null = false
    type = integer
  }
  primary_key {
    columns = [column.assumption_id, column.position]
  }
  foreign_key "0" {
    columns     = [column.series_id]
    ref_columns = [table.event_series.column.id]
    on_update   = NO_ACTION
    on_delete   = CASCADE
  }
  foreign_key "1" {
    columns     = [column.assumption_id]
    ref_columns = [table.assumptions.column.id]
    on_update   = NO_ACTION
    on_delete   = CASCADE
  }
  index "assumption_series_assumption_id_series_id" {
    unique  = true
    columns = [column.assumption_id, column.series_id]
  }
  index "idx_assumption_series_series" {
    columns = [column.series_id]
  }
  check {
    expr = "(position >= 0)"
  }
}
table "scenarios" {
  schema = schema.main
  column "id" {
    null = false
    type = integer
  }
  column "position" {
    null = false
    type = integer
  }
  column "name" {
    null = false
    type = text
  }
  column "description" {
    null = false
    type = text
  }
  column "private_to" {
    null = true
    type = integer
  }
  column "payload_json" {
    null = false
    type = text
  }
  primary_key {
    columns = [column.id]
  }
  foreign_key "0" {
    columns     = [column.private_to]
    ref_columns = [table.people.column.id]
    on_update   = NO_ACTION
    on_delete   = RESTRICT
  }
  index "scenarios_position" {
    unique  = true
    columns = [column.position]
  }
  check {
    expr = "(id >= 0)"
  }
  check {
    expr = "(position >= 0)"
  }
  check {
    expr = "(json_valid(payload_json))"
  }
}
table "scenario_components" {
  schema = schema.main
  column "scenario_id" {
    null = false
    type = integer
  }
  column "position" {
    null = false
    type = integer
  }
  column "component_id" {
    null = false
    type = integer
  }
  primary_key {
    columns = [column.scenario_id, column.position]
  }
  foreign_key "0" {
    columns     = [column.component_id]
    ref_columns = [table.scenarios.column.id]
    on_update   = NO_ACTION
    on_delete   = RESTRICT
  }
  foreign_key "1" {
    columns     = [column.scenario_id]
    ref_columns = [table.scenarios.column.id]
    on_update   = NO_ACTION
    on_delete   = CASCADE
  }
  index "scenario_components_scenario_id_component_id" {
    unique  = true
    columns = [column.scenario_id, column.component_id]
  }
  check {
    expr = "(position >= 0)"
  }
  check {
    expr = "(scenario_id <> component_id)"
  }
}
table "scenario_changes" {
  schema = schema.main
  column "scenario_id" {
    null = false
    type = integer
  }
  column "position" {
    null = false
    type = integer
  }
  column "kind" {
    null = false
    type = text
  }
  column "value_json" {
    null = false
    type = text
  }
  primary_key {
    columns = [column.scenario_id, column.position]
  }
  foreign_key "0" {
    columns     = [column.scenario_id]
    ref_columns = [table.scenarios.column.id]
    on_update   = NO_ACTION
    on_delete   = CASCADE
  }
  check {
    expr = "(position >= 0)"
  }
  check {
    expr = "(json_valid(value_json))"
  }
}
table "tax_packs" {
  schema = schema.main
  column "id" {
    null = false
    type = integer
  }
  column "position" {
    null = false
    type = integer
  }
  column "name" {
    null = false
    type = text
  }
  column "version" {
    null = false
    type = text
  }
  column "jurisdiction" {
    null = false
    type = text
  }
  column "verified" {
    null = false
    type = integer
  }
  column "payload_json" {
    null = false
    type = text
  }
  primary_key {
    columns = [column.id]
  }
  index "tax_packs_position" {
    unique  = true
    columns = [column.position]
  }
  index "tax_packs_name_version_jurisdiction" {
    unique  = true
    columns = [column.name, column.version, column.jurisdiction]
  }
  check {
    expr = "(position >= 0)"
  }
  check {
    expr = "(verified IN (0, 1))"
  }
  check {
    expr = "(json_valid(payload_json))"
  }
}
table "tax_rules" {
  schema = schema.main
  column "id" {
    null = false
    type = integer
  }
  column "pack_id" {
    null = false
    type = integer
  }
  column "position" {
    null = false
    type = integer
  }
  column "name" {
    null = false
    type = text
  }
  column "tax_type" {
    null = false
    type = text
  }
  column "scope" {
    null = false
    type = text
  }
  column "kind_json" {
    null = false
    type = text
  }
  column "timing_json" {
    null = false
    type = text
  }
  column "effective_from" {
    null = false
    type = text
  }
  column "effective_to" {
    null = true
    type = text
  }
  column "source" {
    null = false
    type = text
  }
  column "explanation" {
    null = false
    type = text
  }
  primary_key {
    columns = [column.id]
  }
  foreign_key "0" {
    columns     = [column.pack_id]
    ref_columns = [table.tax_packs.column.id]
    on_update   = NO_ACTION
    on_delete   = CASCADE
  }
  index "tax_rules_pack_id_position" {
    unique  = true
    columns = [column.pack_id, column.position]
  }
  index "idx_tax_rules_pack" {
    columns = [column.pack_id, column.position]
  }
  check {
    expr = "(id >= 0)"
  }
  check {
    expr = "(position >= 0)"
  }
  check {
    expr = "(json_valid(kind_json))"
  }
  check {
    expr = "(json_valid(timing_json))"
  }
  check {
    expr = "(length(effective_from) = 10)"
  }
  check {
    expr = "(effective_to IS NULL OR length(effective_to) = 10)"
  }
}
table "tax_rule_categories" {
  schema = schema.main
  column "tax_rule_id" {
    null = false
    type = integer
  }
  column "position" {
    null = false
    type = integer
  }
  column "category" {
    null = false
    type = text
  }
  primary_key {
    columns = [column.tax_rule_id, column.position]
  }
  foreign_key "0" {
    columns     = [column.tax_rule_id]
    ref_columns = [table.tax_rules.column.id]
    on_update   = NO_ACTION
    on_delete   = CASCADE
  }
  index "tax_rule_categories_tax_rule_id_category" {
    unique  = true
    columns = [column.tax_rule_id, column.category]
  }
  check {
    expr = "(position >= 0)"
  }
}
table "access_policies" {
  schema = schema.main
  column "id" {
    null = false
    type = integer
  }
  column "position" {
    null = false
    type = integer
  }
  column "object_json" {
    null = false
    type = text
  }
  column "calculation_access" {
    null = false
    type = text
  }
  column "restricted_disclosure" {
    null = false
    type = text
  }
  column "effective_from" {
    null = false
    type = text
  }
  column "version" {
    null = false
    type = integer
  }
  column "changed_by" {
    null = false
    type = integer
  }
  column "changed_at" {
    null = false
    type = text
  }
  column "payload_json" {
    null = false
    type = text
  }
  primary_key {
    columns = [column.id]
  }
  foreign_key "0" {
    columns     = [column.changed_by]
    ref_columns = [table.people.column.id]
    on_update   = NO_ACTION
    on_delete   = RESTRICT
  }
  index "access_policies_position" {
    unique  = true
    columns = [column.position]
  }
  index "idx_access_policies_changed_by" {
    columns = [column.changed_by]
  }
  check {
    expr = "(id >= 0)"
  }
  check {
    expr = "(position >= 0)"
  }
  check {
    expr = "(json_valid(object_json))"
  }
  check {
    expr = "(length(effective_from) = 10)"
  }
  check {
    expr = "(version > 0)"
  }
  check {
    expr = "(json_valid(payload_json))"
  }
}
table "policy_full_access" {
  schema = schema.main
  column "policy_id" {
    null = false
    type = integer
  }
  column "position" {
    null = false
    type = integer
  }
  column "person_id" {
    null = false
    type = integer
  }
  primary_key {
    columns = [column.policy_id, column.position]
  }
  foreign_key "0" {
    columns     = [column.person_id]
    ref_columns = [table.people.column.id]
    on_update   = NO_ACTION
    on_delete   = RESTRICT
  }
  foreign_key "1" {
    columns     = [column.policy_id]
    ref_columns = [table.access_policies.column.id]
    on_update   = NO_ACTION
    on_delete   = CASCADE
  }
  index "policy_full_access_policy_id_person_id" {
    unique  = true
    columns = [column.policy_id, column.person_id]
  }
  check {
    expr = "(position >= 0)"
  }
}
table "policy_purposes" {
  schema = schema.main
  column "policy_id" {
    null = false
    type = integer
  }
  column "position" {
    null = false
    type = integer
  }
  column "purpose" {
    null = false
    type = text
  }
  primary_key {
    columns = [column.policy_id, column.position]
  }
  foreign_key "0" {
    columns     = [column.policy_id]
    ref_columns = [table.access_policies.column.id]
    on_update   = NO_ACTION
    on_delete   = CASCADE
  }
  index "policy_purposes_policy_id_purpose" {
    unique  = true
    columns = [column.policy_id, column.purpose]
  }
  check {
    expr = "(position >= 0)"
  }
}
table "actual_transactions" {
  schema = schema.main
  column "id" {
    null = false
    type = integer
  }
  column "position" {
    null = false
    type = integer
  }
  column "transaction_date" {
    null = false
    type = text
  }
  column "account_id" {
    null = false
    type = integer
  }
  column "amount_minor" {
    null = false
    type = integer
  }
  column "currency" {
    null = false
    type = text
  }
  column "description" {
    null = false
    type = text
  }
  column "payload_json" {
    null = false
    type = text
  }
  primary_key {
    columns = [column.id]
  }
  foreign_key "0" {
    columns     = [column.account_id]
    ref_columns = [table.accounts.column.id]
    on_update   = NO_ACTION
    on_delete   = RESTRICT
  }
  index "actual_transactions_position" {
    unique  = true
    columns = [column.position]
  }
  index "idx_actual_transactions_account_date" {
    columns = [column.account_id, column.transaction_date]
  }
  check {
    expr = "(id >= 0)"
  }
  check {
    expr = "(position >= 0)"
  }
  check {
    expr = "(length(transaction_date) = 10)"
  }
  check {
    expr = "(length(currency) = 3)"
  }
  check {
    expr = "(json_valid(payload_json))"
  }
}
table "reconciliation_links" {
  schema = schema.main
  column "position" {
    null = false
    type = integer
  }
  column "series_id" {
    null = false
    type = integer
  }
  column "original_due" {
    null = false
    type = text
  }
  column "transaction_id" {
    null = false
    type = integer
  }
  column "amount_minor" {
    null = false
    type = integer
  }
  column "currency" {
    null = false
    type = text
  }
  column "payload_json" {
    null = false
    type = text
  }
  primary_key {
    columns = [column.position]
  }
  foreign_key "0" {
    columns     = [column.transaction_id]
    ref_columns = [table.actual_transactions.column.id]
    on_update   = NO_ACTION
    on_delete   = CASCADE
  }
  foreign_key "1" {
    columns     = [column.series_id]
    ref_columns = [table.event_series.column.id]
    on_update   = NO_ACTION
    on_delete   = RESTRICT
  }
  index "reconciliation_links_series_id_original_due_transaction_id" {
    unique  = true
    columns = [column.series_id, column.original_due, column.transaction_id]
  }
  index "idx_reconciliation_links_occurrence" {
    columns = [column.series_id, column.original_due]
  }
  index "idx_reconciliation_links_transaction" {
    columns = [column.transaction_id]
  }
  check {
    expr = "(position >= 0)"
  }
  check {
    expr = "(length(original_due) = 10)"
  }
  check {
    expr = "(amount_minor >= 0)"
  }
  check {
    expr = "(length(currency) = 3)"
  }
  check {
    expr = "(json_valid(payload_json))"
  }
}
table "historical_payments" {
  schema = schema.main
  column "position" {
    null = false
    type = integer
  }
  column "series_id" {
    null = false
    type = integer
  }
  column "payment_date" {
    null = false
    type = text
  }
  column "amount_minor" {
    null = false
    type = integer
  }
  column "currency" {
    null = false
    type = text
  }
  column "payload_json" {
    null = false
    type = text
  }
  primary_key {
    columns = [column.position]
  }
  foreign_key "0" {
    columns     = [column.series_id]
    ref_columns = [table.event_series.column.id]
    on_update   = NO_ACTION
    on_delete   = CASCADE
  }
  index "idx_historical_payments_series_date" {
    columns = [column.series_id, column.payment_date]
  }
  check {
    expr = "(position >= 0)"
  }
  check {
    expr = "(length(payment_date) = 10)"
  }
  check {
    expr = "(amount_minor >= 0)"
  }
  check {
    expr = "(length(currency) = 3)"
  }
  check {
    expr = "(json_valid(payload_json))"
  }
}
table "rules" {
  schema = schema.main
  column "id" {
    null = false
    type = integer
  }
  column "position" {
    null = false
    type = integer
  }
  column "name" {
    null = false
    type = text
  }
  column "scope_json" {
    null = false
    type = text
  }
  column "trigger_json" {
    null = false
    type = text
  }
  column "action_json" {
    null = false
    type = text
  }
  column "priority" {
    null = false
    type = integer
  }
  column "effective_from" {
    null = false
    type = text
  }
  column "effective_to" {
    null = true
    type = text
  }
  column "enabled" {
    null = false
    type = integer
  }
  column "scenario_id" {
    null = true
    type = integer
  }
  column "explanation" {
    null = false
    type = text
  }
  column "version" {
    null = false
    type = integer
  }
  column "payload_json" {
    null = false
    type = text
  }
  primary_key {
    columns = [column.id]
  }
  foreign_key "0" {
    columns     = [column.scenario_id]
    ref_columns = [table.scenarios.column.id]
    on_update   = NO_ACTION
    on_delete   = RESTRICT
  }
  index "rules_position" {
    unique  = true
    columns = [column.position]
  }
  index "idx_rules_enabled_dates" {
    columns = [column.enabled, column.effective_from, column.effective_to]
  }
  index "idx_rules_scenario" {
    columns = [column.scenario_id]
  }
  check {
    expr = "(id >= 0)"
  }
  check {
    expr = "(position >= 0)"
  }
  check {
    expr = "(json_valid(scope_json))"
  }
  check {
    expr = "(json_valid(trigger_json))"
  }
  check {
    expr = "(json_valid(action_json))"
  }
  check {
    expr = "(length(effective_from) = 10)"
  }
  check {
    expr = "(effective_to IS NULL OR length(effective_to) = 10)"
  }
  check {
    expr = "(enabled IN (0, 1))"
  }
  check {
    expr = "(version > 0)"
  }
  check {
    expr = "(json_valid(payload_json))"
  }
}
table "rule_conditions" {
  schema = schema.main
  column "rule_id" {
    null = false
    type = integer
  }
  column "position" {
    null = false
    type = integer
  }
  column "value_json" {
    null = false
    type = text
  }
  primary_key {
    columns = [column.rule_id, column.position]
  }
  foreign_key "0" {
    columns     = [column.rule_id]
    ref_columns = [table.rules.column.id]
    on_update   = NO_ACTION
    on_delete   = CASCADE
  }
  check {
    expr = "(position >= 0)"
  }
  check {
    expr = "(json_valid(value_json))"
  }
}
table "rule_versions" {
  schema = schema.main
  column "rule_id" {
    null = false
    type = integer
  }
  column "position" {
    null = false
    type = integer
  }
  column "version" {
    null = false
    type = integer
  }
  column "changed_on" {
    null = false
    type = text
  }
  column "summary" {
    null = false
    type = text
  }
  primary_key {
    columns = [column.rule_id, column.position]
  }
  foreign_key "0" {
    columns     = [column.rule_id]
    ref_columns = [table.rules.column.id]
    on_update   = NO_ACTION
    on_delete   = CASCADE
  }
  index "rule_versions_rule_id_version" {
    unique  = true
    columns = [column.rule_id, column.version]
  }
  check {
    expr = "(position >= 0)"
  }
  check {
    expr = "(version > 0)"
  }
  check {
    expr = "(length(changed_on) = 10)"
  }
}
table "goals" {
  schema = schema.main
  column "id" {
    null = false
    type = integer
  }
  column "position" {
    null = false
    type = integer
  }
  column "name" {
    null = false
    type = text
  }
  column "amount_minor" {
    null = false
    type = integer
  }
  column "currency" {
    null = false
    type = text
  }
  column "target_on" {
    null = false
    type = text
  }
  column "priority" {
    null = false
    type = integer
  }
  column "private_to" {
    null = true
    type = integer
  }
  column "payload_json" {
    null = false
    type = text
  }
  primary_key {
    columns = [column.id]
  }
  foreign_key "0" {
    columns     = [column.private_to]
    ref_columns = [table.people.column.id]
    on_update   = NO_ACTION
    on_delete   = RESTRICT
  }
  index "goals_position" {
    unique  = true
    columns = [column.position]
  }
  index "idx_goals_target_priority" {
    columns = [column.target_on, column.priority]
  }
  check {
    expr = "(id >= 0)"
  }
  check {
    expr = "(position >= 0)"
  }
  check {
    expr = "(amount_minor >= 0)"
  }
  check {
    expr = "(length(currency) = 3)"
  }
  check {
    expr = "(length(target_on) = 10)"
  }
  check {
    expr = "(priority BETWEEN 0 AND 255)"
  }
  check {
    expr = "(json_valid(payload_json))"
  }
}
table "access_grants" {
  schema = schema.main
  column "id" {
    null = false
    type = integer
  }
  column "position" {
    null = false
    type = integer
  }
  column "object_json" {
    null = false
    type = text
  }
  column "grantee_json" {
    null = false
    type = text
  }
  column "purpose_json" {
    null = false
    type = text
  }
  column "disclosure" {
    null = false
    type = text
  }
  column "calculation_access" {
    null = false
    type = text
  }
  column "effective_from" {
    null = false
    type = text
  }
  column "effective_to" {
    null = true
    type = text
  }
  column "granted_by" {
    null = false
    type = integer
  }
  column "granted_at" {
    null = false
    type = text
  }
  column "revoked_on" {
    null = true
    type = text
  }
  column "note" {
    null = false
    type = text
  }
  column "payload_json" {
    null = false
    type = text
  }
  primary_key {
    columns = [column.id]
  }
  foreign_key "0" {
    columns     = [column.granted_by]
    ref_columns = [table.people.column.id]
    on_update   = NO_ACTION
    on_delete   = RESTRICT
  }
  index "access_grants_position" {
    unique  = true
    columns = [column.position]
  }
  index "idx_access_grants_granted_by" {
    columns = [column.granted_by]
  }
  index "idx_access_grants_dates" {
    columns = [column.effective_from, column.effective_to, column.revoked_on]
  }
  check {
    expr = "(id >= 0)"
  }
  check {
    expr = "(position >= 0)"
  }
  check {
    expr = "(json_valid(object_json))"
  }
  check {
    expr = "(json_valid(grantee_json))"
  }
  check {
    expr = "(json_valid(purpose_json))"
  }
  check {
    expr = "(length(effective_from) = 10)"
  }
  check {
    expr = "(effective_to IS NULL OR length(effective_to) = 10)"
  }
  check {
    expr = "(revoked_on IS NULL OR length(revoked_on) = 10)"
  }
  check {
    expr = "(json_valid(payload_json))"
  }
}
table "audit_events" {
  schema = schema.main
  column "id" {
    null = false
    type = integer
  }
  column "position" {
    null = false
    type = integer
  }
  column "occurred_at" {
    null = false
    type = text
  }
  column "actor_id" {
    null = false
    type = integer
  }
  column "object_json" {
    null = true
    type = text
  }
  column "kind_json" {
    null = false
    type = text
  }
  column "summary" {
    null = false
    type = text
  }
  column "policy_version" {
    null = true
    type = integer
  }
  column "payload_json" {
    null = false
    type = text
  }
  primary_key {
    columns = [column.id]
  }
  foreign_key "0" {
    columns     = [column.actor_id]
    ref_columns = [table.people.column.id]
    on_update   = NO_ACTION
    on_delete   = RESTRICT
  }
  index "audit_events_position" {
    unique  = true
    columns = [column.position]
  }
  index "idx_audit_events_actor_time" {
    columns = [column.actor_id, column.occurred_at]
  }
  check {
    expr = "(id >= 0)"
  }
  check {
    expr = "(position >= 0)"
  }
  check {
    expr = "(object_json IS NULL OR json_valid(object_json))"
  }
  check {
    expr = "(json_valid(kind_json))"
  }
  check {
    expr = "(policy_version IS NULL OR policy_version > 0)"
  }
  check {
    expr = "(json_valid(payload_json))"
  }
}
schema "main" {
}
