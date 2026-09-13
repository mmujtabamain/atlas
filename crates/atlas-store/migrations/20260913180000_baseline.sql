-- Atlas Financer normalized persistence baseline.
-- Complex Rust sum types remain as checked JSON value objects; identities,
-- ordering, relationships, money, dates, and searchable fields are relational.

CREATE TABLE households (
  singleton INTEGER NOT NULL PRIMARY KEY CHECK (singleton = 1),
  household_id TEXT NOT NULL UNIQUE,
  name TEXT NOT NULL,
  base_currency TEXT NOT NULL CHECK (length(base_currency) = 3),
  as_of TEXT NOT NULL CHECK (length(as_of) = 10),
  rule_tie_break TEXT NOT NULL CHECK (rule_tie_break IN ('oldest-rule', 'newest-version')),
  last_saved_at INTEGER NOT NULL,
  last_written_by_version TEXT NOT NULL,
  format_lineage TEXT NOT NULL
);

CREATE TABLE people (
  id INTEGER NOT NULL PRIMARY KEY CHECK (id >= 0),
  position INTEGER NOT NULL UNIQUE CHECK (position >= 0),
  name TEXT NOT NULL,
  role TEXT NOT NULL CHECK (role IN ('owner', 'member', 'dependent', 'adviser', 'read_only')),
  payload_json TEXT NOT NULL CHECK (json_valid(payload_json))
);

CREATE TABLE companies (
  id INTEGER NOT NULL PRIMARY KEY CHECK (id >= 0),
  position INTEGER NOT NULL UNIQUE CHECK (position >= 0),
  name TEXT NOT NULL,
  jurisdiction TEXT NOT NULL,
  payload_json TEXT NOT NULL CHECK (json_valid(payload_json))
);

CREATE TABLE company_owners (
  company_id INTEGER NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  position INTEGER NOT NULL CHECK (position >= 0),
  person_id INTEGER NOT NULL REFERENCES people(id) ON DELETE RESTRICT,
  basis_points INTEGER NOT NULL CHECK (basis_points BETWEEN 0 AND 10000),
  PRIMARY KEY (company_id, position),
  UNIQUE (company_id, person_id)
);
CREATE INDEX idx_company_owners_person ON company_owners(person_id);

CREATE TABLE company_roles (
  company_id INTEGER NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  position INTEGER NOT NULL CHECK (position >= 0),
  person_id INTEGER NOT NULL REFERENCES people(id) ON DELETE RESTRICT,
  role TEXT NOT NULL,
  PRIMARY KEY (company_id, position),
  UNIQUE (company_id, person_id, role)
);
CREATE INDEX idx_company_roles_person ON company_roles(person_id);

CREATE TABLE company_employees (
  company_id INTEGER NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  position INTEGER NOT NULL CHECK (position >= 0),
  person_id INTEGER REFERENCES people(id) ON DELETE RESTRICT,
  name TEXT NOT NULL,
  monthly_gross_minor INTEGER NOT NULL,
  currency TEXT NOT NULL CHECK (length(currency) = 3),
  starts_on TEXT NOT NULL CHECK (length(starts_on) = 10),
  ends_on TEXT CHECK (ends_on IS NULL OR length(ends_on) = 10),
  PRIMARY KEY (company_id, position)
);
CREATE INDEX idx_company_employees_person ON company_employees(person_id);

CREATE TABLE company_constraints (
  company_id INTEGER NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  position INTEGER NOT NULL CHECK (position >= 0),
  kind TEXT NOT NULL,
  value_json TEXT NOT NULL CHECK (json_valid(value_json)),
  PRIMARY KEY (company_id, position)
);

CREATE TABLE accounts (
  id INTEGER NOT NULL PRIMARY KEY CHECK (id >= 0),
  position INTEGER NOT NULL UNIQUE CHECK (position >= 0),
  name TEXT NOT NULL,
  institution TEXT NOT NULL,
  kind TEXT NOT NULL,
  holder_kind TEXT NOT NULL CHECK (holder_kind IN ('persons', 'company')),
  holder_company_id INTEGER REFERENCES companies(id) ON DELETE RESTRICT,
  currency TEXT NOT NULL CHECK (length(currency) = 3),
  liquidity_json TEXT NOT NULL CHECK (json_valid(liquidity_json)),
  minimum_balance_minor INTEGER,
  minimum_balance_currency TEXT CHECK (minimum_balance_currency IS NULL OR length(minimum_balance_currency) = 3),
  transfer_delay_days INTEGER NOT NULL CHECK (transfer_delay_days BETWEEN 0 AND 65535),
  tax_treatment TEXT NOT NULL,
  source_of_truth TEXT NOT NULL,
  last_reconciled TEXT CHECK (last_reconciled IS NULL OR length(last_reconciled) = 10),
  withdrawals_permitted INTEGER NOT NULL CHECK (withdrawals_permitted IN (0, 1)),
  include_in_household INTEGER NOT NULL CHECK (include_in_household IN (0, 1)),
  settled_balance_minor INTEGER NOT NULL,
  pending_balance_minor INTEGER NOT NULL,
  payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
  CHECK ((holder_kind = 'company' AND holder_company_id IS NOT NULL) OR (holder_kind = 'persons' AND holder_company_id IS NULL))
);
CREATE INDEX idx_accounts_company ON accounts(holder_company_id);
CREATE INDEX idx_accounts_kind ON accounts(kind);

CREATE TABLE account_owners (
  account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  position INTEGER NOT NULL CHECK (position >= 0),
  person_id INTEGER NOT NULL REFERENCES people(id) ON DELETE RESTRICT,
  basis_points INTEGER NOT NULL CHECK (basis_points BETWEEN 0 AND 10000),
  PRIMARY KEY (account_id, position),
  UNIQUE (account_id, person_id)
);
CREATE INDEX idx_account_owners_person ON account_owners(person_id);

CREATE TABLE account_fees (
  account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  position INTEGER NOT NULL CHECK (position >= 0),
  description TEXT NOT NULL,
  fixed_minor INTEGER,
  fixed_currency TEXT CHECK (fixed_currency IS NULL OR length(fixed_currency) = 3),
  basis_points INTEGER NOT NULL CHECK (basis_points >= 0),
  PRIMARY KEY (account_id, position)
);

CREATE TABLE account_fund_categories (
  account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  position INTEGER NOT NULL CHECK (position >= 0),
  category TEXT NOT NULL,
  PRIMARY KEY (account_id, position),
  UNIQUE (account_id, category)
);

CREATE TABLE reservations (
  id INTEGER NOT NULL PRIMARY KEY CHECK (id >= 0),
  position INTEGER NOT NULL UNIQUE CHECK (position >= 0),
  name TEXT NOT NULL,
  account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE RESTRICT,
  amount_minor INTEGER NOT NULL CHECK (amount_minor >= 0),
  currency TEXT NOT NULL CHECK (length(currency) = 3),
  coverage_json TEXT NOT NULL CHECK (json_valid(coverage_json)),
  nested_in_id INTEGER REFERENCES reservations(id) ON DELETE RESTRICT,
  hardness TEXT NOT NULL CHECK (hardness IN ('hard', 'soft_user_relaxable')),
  purpose TEXT NOT NULL,
  released_on TEXT CHECK (released_on IS NULL OR length(released_on) = 10),
  payload_json TEXT NOT NULL CHECK (json_valid(payload_json))
);
CREATE INDEX idx_reservations_account ON reservations(account_id, released_on);
CREATE INDEX idx_reservations_nested ON reservations(nested_in_id);

CREATE TABLE event_series (
  id INTEGER NOT NULL PRIMARY KEY CHECK (id >= 0),
  position INTEGER NOT NULL UNIQUE CHECK (position >= 0),
  name TEXT NOT NULL,
  direction_json TEXT NOT NULL CHECK (json_valid(direction_json)),
  expected_minor INTEGER NOT NULL CHECK (expected_minor >= 0),
  currency TEXT NOT NULL CHECK (length(currency) = 3),
  recurrence_json TEXT NOT NULL CHECK (json_valid(recurrence_json)),
  settlement_lag_days INTEGER NOT NULL CHECK (settlement_lag_days BETWEEN 0 AND 65535),
  availability_lag_days INTEGER NOT NULL CHECK (availability_lag_days BETWEEN 0 AND 65535),
  intraday_order INTEGER NOT NULL CHECK (intraday_order BETWEEN 0 AND 65535),
  account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE RESTRICT,
  linked_account_id INTEGER REFERENCES accounts(id) ON DELETE RESTRICT,
  entity_json TEXT NOT NULL CHECK (json_valid(entity_json)),
  certainty TEXT NOT NULL,
  category TEXT NOT NULL,
  tax_treatment TEXT NOT NULL,
  scenario_id INTEGER,
  notes TEXT NOT NULL,
  payload_json TEXT NOT NULL CHECK (json_valid(payload_json))
);
CREATE INDEX idx_event_series_account ON event_series(account_id, position);
CREATE INDEX idx_event_series_linked_account ON event_series(linked_account_id);
CREATE INDEX idx_event_series_scenario ON event_series(scenario_id);

CREATE TABLE series_amount_changes (
  series_id INTEGER NOT NULL REFERENCES event_series(id) ON DELETE CASCADE,
  position INTEGER NOT NULL CHECK (position >= 0),
  effective_from TEXT NOT NULL CHECK (length(effective_from) = 10),
  amount_json TEXT NOT NULL CHECK (json_valid(amount_json)),
  PRIMARY KEY (series_id, position)
);

CREATE TABLE series_exceptions (
  series_id INTEGER NOT NULL REFERENCES event_series(id) ON DELETE CASCADE,
  position INTEGER NOT NULL CHECK (position >= 0),
  original_due TEXT NOT NULL CHECK (length(original_due) = 10),
  kind_json TEXT NOT NULL CHECK (json_valid(kind_json)),
  PRIMARY KEY (series_id, position),
  UNIQUE (series_id, original_due)
);

CREATE TABLE assumptions (
  id INTEGER NOT NULL PRIMARY KEY CHECK (id >= 0),
  position INTEGER NOT NULL UNIQUE CHECK (position >= 0),
  text TEXT NOT NULL,
  certainty TEXT NOT NULL,
  source_json TEXT NOT NULL CHECK (json_valid(source_json)),
  accepted_on TEXT CHECK (accepted_on IS NULL OR length(accepted_on) = 10),
  expires_on TEXT CHECK (expires_on IS NULL OR length(expires_on) = 10),
  private_to INTEGER REFERENCES people(id) ON DELETE RESTRICT,
  payload_json TEXT NOT NULL CHECK (json_valid(payload_json))
);
CREATE INDEX idx_assumptions_private_to ON assumptions(private_to);

CREATE TABLE assumption_series (
  assumption_id INTEGER NOT NULL REFERENCES assumptions(id) ON DELETE CASCADE,
  position INTEGER NOT NULL CHECK (position >= 0),
  series_id INTEGER NOT NULL REFERENCES event_series(id) ON DELETE CASCADE,
  PRIMARY KEY (assumption_id, position),
  UNIQUE (assumption_id, series_id)
);
CREATE INDEX idx_assumption_series_series ON assumption_series(series_id);

CREATE TABLE scenarios (
  id INTEGER NOT NULL PRIMARY KEY CHECK (id >= 0),
  position INTEGER NOT NULL UNIQUE CHECK (position >= 0),
  name TEXT NOT NULL,
  description TEXT NOT NULL,
  private_to INTEGER REFERENCES people(id) ON DELETE RESTRICT,
  payload_json TEXT NOT NULL CHECK (json_valid(payload_json))
);

CREATE TABLE scenario_components (
  scenario_id INTEGER NOT NULL REFERENCES scenarios(id) ON DELETE CASCADE,
  position INTEGER NOT NULL CHECK (position >= 0),
  component_id INTEGER NOT NULL REFERENCES scenarios(id) ON DELETE RESTRICT,
  PRIMARY KEY (scenario_id, position),
  UNIQUE (scenario_id, component_id),
  CHECK (scenario_id <> component_id)
);

CREATE TABLE scenario_changes (
  scenario_id INTEGER NOT NULL REFERENCES scenarios(id) ON DELETE CASCADE,
  position INTEGER NOT NULL CHECK (position >= 0),
  kind TEXT NOT NULL,
  value_json TEXT NOT NULL CHECK (json_valid(value_json)),
  PRIMARY KEY (scenario_id, position)
);

CREATE TABLE tax_packs (
  id INTEGER NOT NULL PRIMARY KEY,
  position INTEGER NOT NULL UNIQUE CHECK (position >= 0),
  name TEXT NOT NULL,
  version TEXT NOT NULL,
  jurisdiction TEXT NOT NULL,
  verified INTEGER NOT NULL CHECK (verified IN (0, 1)),
  payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
  UNIQUE (name, version, jurisdiction)
);

CREATE TABLE tax_rules (
  id INTEGER NOT NULL PRIMARY KEY CHECK (id >= 0),
  pack_id INTEGER NOT NULL REFERENCES tax_packs(id) ON DELETE CASCADE,
  position INTEGER NOT NULL CHECK (position >= 0),
  name TEXT NOT NULL,
  tax_type TEXT NOT NULL,
  scope TEXT NOT NULL,
  kind_json TEXT NOT NULL CHECK (json_valid(kind_json)),
  timing_json TEXT NOT NULL CHECK (json_valid(timing_json)),
  effective_from TEXT NOT NULL CHECK (length(effective_from) = 10),
  effective_to TEXT CHECK (effective_to IS NULL OR length(effective_to) = 10),
  source TEXT NOT NULL,
  explanation TEXT NOT NULL,
  UNIQUE (pack_id, position)
);
CREATE INDEX idx_tax_rules_pack ON tax_rules(pack_id, position);

CREATE TABLE tax_rule_categories (
  tax_rule_id INTEGER NOT NULL REFERENCES tax_rules(id) ON DELETE CASCADE,
  position INTEGER NOT NULL CHECK (position >= 0),
  category TEXT NOT NULL,
  PRIMARY KEY (tax_rule_id, position),
  UNIQUE (tax_rule_id, category)
);

CREATE TABLE access_policies (
  id INTEGER NOT NULL PRIMARY KEY CHECK (id >= 0),
  position INTEGER NOT NULL UNIQUE CHECK (position >= 0),
  object_json TEXT NOT NULL CHECK (json_valid(object_json)),
  calculation_access TEXT NOT NULL,
  restricted_disclosure TEXT NOT NULL,
  effective_from TEXT NOT NULL CHECK (length(effective_from) = 10),
  version INTEGER NOT NULL CHECK (version > 0),
  changed_by INTEGER NOT NULL REFERENCES people(id) ON DELETE RESTRICT,
  changed_at TEXT NOT NULL,
  payload_json TEXT NOT NULL CHECK (json_valid(payload_json))
);
CREATE INDEX idx_access_policies_changed_by ON access_policies(changed_by);

CREATE TABLE policy_full_access (
  policy_id INTEGER NOT NULL REFERENCES access_policies(id) ON DELETE CASCADE,
  position INTEGER NOT NULL CHECK (position >= 0),
  person_id INTEGER NOT NULL REFERENCES people(id) ON DELETE RESTRICT,
  PRIMARY KEY (policy_id, position),
  UNIQUE (policy_id, person_id)
);

CREATE TABLE policy_purposes (
  policy_id INTEGER NOT NULL REFERENCES access_policies(id) ON DELETE CASCADE,
  position INTEGER NOT NULL CHECK (position >= 0),
  purpose TEXT NOT NULL,
  PRIMARY KEY (policy_id, position),
  UNIQUE (policy_id, purpose)
);

CREATE TABLE actual_transactions (
  id INTEGER NOT NULL PRIMARY KEY CHECK (id >= 0),
  position INTEGER NOT NULL UNIQUE CHECK (position >= 0),
  transaction_date TEXT NOT NULL CHECK (length(transaction_date) = 10),
  account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE RESTRICT,
  amount_minor INTEGER NOT NULL,
  currency TEXT NOT NULL CHECK (length(currency) = 3),
  description TEXT NOT NULL,
  payload_json TEXT NOT NULL CHECK (json_valid(payload_json))
);
CREATE INDEX idx_actual_transactions_account_date ON actual_transactions(account_id, transaction_date);

CREATE TABLE reconciliation_links (
  position INTEGER NOT NULL PRIMARY KEY CHECK (position >= 0),
  series_id INTEGER NOT NULL REFERENCES event_series(id) ON DELETE RESTRICT,
  original_due TEXT NOT NULL CHECK (length(original_due) = 10),
  transaction_id INTEGER NOT NULL REFERENCES actual_transactions(id) ON DELETE CASCADE,
  amount_minor INTEGER NOT NULL CHECK (amount_minor >= 0),
  currency TEXT NOT NULL CHECK (length(currency) = 3),
  payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
  UNIQUE (series_id, original_due, transaction_id)
);
CREATE INDEX idx_reconciliation_links_occurrence ON reconciliation_links(series_id, original_due);
CREATE INDEX idx_reconciliation_links_transaction ON reconciliation_links(transaction_id);

CREATE TABLE historical_payments (
  position INTEGER NOT NULL PRIMARY KEY CHECK (position >= 0),
  series_id INTEGER NOT NULL REFERENCES event_series(id) ON DELETE CASCADE,
  payment_date TEXT NOT NULL CHECK (length(payment_date) = 10),
  amount_minor INTEGER NOT NULL CHECK (amount_minor >= 0),
  currency TEXT NOT NULL CHECK (length(currency) = 3),
  payload_json TEXT NOT NULL CHECK (json_valid(payload_json))
);
CREATE INDEX idx_historical_payments_series_date ON historical_payments(series_id, payment_date);

CREATE TABLE rules (
  id INTEGER NOT NULL PRIMARY KEY CHECK (id >= 0),
  position INTEGER NOT NULL UNIQUE CHECK (position >= 0),
  name TEXT NOT NULL,
  scope_json TEXT NOT NULL CHECK (json_valid(scope_json)),
  trigger_json TEXT NOT NULL CHECK (json_valid(trigger_json)),
  action_json TEXT NOT NULL CHECK (json_valid(action_json)),
  priority INTEGER NOT NULL,
  effective_from TEXT NOT NULL CHECK (length(effective_from) = 10),
  effective_to TEXT CHECK (effective_to IS NULL OR length(effective_to) = 10),
  enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
  scenario_id INTEGER REFERENCES scenarios(id) ON DELETE RESTRICT,
  explanation TEXT NOT NULL,
  version INTEGER NOT NULL CHECK (version > 0),
  payload_json TEXT NOT NULL CHECK (json_valid(payload_json))
);
CREATE INDEX idx_rules_enabled_dates ON rules(enabled, effective_from, effective_to);
CREATE INDEX idx_rules_scenario ON rules(scenario_id);

CREATE TABLE rule_conditions (
  rule_id INTEGER NOT NULL REFERENCES rules(id) ON DELETE CASCADE,
  position INTEGER NOT NULL CHECK (position >= 0),
  value_json TEXT NOT NULL CHECK (json_valid(value_json)),
  PRIMARY KEY (rule_id, position)
);

CREATE TABLE rule_versions (
  rule_id INTEGER NOT NULL REFERENCES rules(id) ON DELETE CASCADE,
  position INTEGER NOT NULL CHECK (position >= 0),
  version INTEGER NOT NULL CHECK (version > 0),
  changed_on TEXT NOT NULL CHECK (length(changed_on) = 10),
  summary TEXT NOT NULL,
  PRIMARY KEY (rule_id, position),
  UNIQUE (rule_id, version)
);

CREATE TABLE goals (
  id INTEGER NOT NULL PRIMARY KEY CHECK (id >= 0),
  position INTEGER NOT NULL UNIQUE CHECK (position >= 0),
  name TEXT NOT NULL,
  amount_minor INTEGER NOT NULL CHECK (amount_minor >= 0),
  currency TEXT NOT NULL CHECK (length(currency) = 3),
  target_on TEXT NOT NULL CHECK (length(target_on) = 10),
  priority INTEGER NOT NULL CHECK (priority BETWEEN 0 AND 255),
  private_to INTEGER REFERENCES people(id) ON DELETE RESTRICT,
  payload_json TEXT NOT NULL CHECK (json_valid(payload_json))
);
CREATE INDEX idx_goals_target_priority ON goals(target_on, priority);

CREATE TABLE access_grants (
  id INTEGER NOT NULL PRIMARY KEY CHECK (id >= 0),
  position INTEGER NOT NULL UNIQUE CHECK (position >= 0),
  object_json TEXT NOT NULL CHECK (json_valid(object_json)),
  grantee_json TEXT NOT NULL CHECK (json_valid(grantee_json)),
  purpose_json TEXT NOT NULL CHECK (json_valid(purpose_json)),
  disclosure TEXT NOT NULL,
  calculation_access TEXT NOT NULL,
  effective_from TEXT NOT NULL CHECK (length(effective_from) = 10),
  effective_to TEXT CHECK (effective_to IS NULL OR length(effective_to) = 10),
  granted_by INTEGER NOT NULL REFERENCES people(id) ON DELETE RESTRICT,
  granted_at TEXT NOT NULL,
  revoked_on TEXT CHECK (revoked_on IS NULL OR length(revoked_on) = 10),
  note TEXT NOT NULL,
  payload_json TEXT NOT NULL CHECK (json_valid(payload_json))
);
CREATE INDEX idx_access_grants_granted_by ON access_grants(granted_by);
CREATE INDEX idx_access_grants_dates ON access_grants(effective_from, effective_to, revoked_on);

CREATE TABLE audit_events (
  id INTEGER NOT NULL PRIMARY KEY CHECK (id >= 0),
  position INTEGER NOT NULL UNIQUE CHECK (position >= 0),
  occurred_at TEXT NOT NULL,
  actor_id INTEGER NOT NULL REFERENCES people(id) ON DELETE RESTRICT,
  object_json TEXT CHECK (object_json IS NULL OR json_valid(object_json)),
  kind_json TEXT NOT NULL CHECK (json_valid(kind_json)),
  summary TEXT NOT NULL,
  policy_version INTEGER CHECK (policy_version IS NULL OR policy_version > 0),
  payload_json TEXT NOT NULL CHECK (json_valid(payload_json))
);
CREATE INDEX idx_audit_events_actor_time ON audit_events(actor_id, occurred_at);
