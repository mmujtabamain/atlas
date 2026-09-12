//! Derived models for the People, Companies and Accounts screens, computed
//! once per state change and projected for the viewer.

use atlas_core::authz::Viewer;
use atlas_core::ids::*;
use atlas_core::liquidity::{account_liquidity, company_cash, person_attribution};
use atlas_core::model::Household;
use atlas_core::timeline::Direction;
use atlas_core::{Disclosure, EngineResult};

use crate::widgets::figure::ExplainedFigure;

#[derive(Clone, Debug)]
pub struct PersonModel {
    pub id: PersonId,
    pub attribution: ExplainedFigure,
    /// Income series attributed to the person.
    pub income: Vec<SeriesId>,
    /// Tax cash the person owes in the window, as text.
    pub tax_text: String,
}

#[derive(Clone, Debug)]
pub struct CompanyModel {
    pub id: CompanyId,
    pub disclosure: Disclosure,
    pub cash: ExplainedFigure,
    pub committed: ExplainedFigure,
    pub ceiling: ExplainedFigure,
    pub extractable: ExplainedFigure,
}

/// How a series touches an account.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Touch {
    PostsHere,
    LinkedSide,
    TransferTarget,
}

impl Touch {
    pub fn label(self) -> &'static str {
        match self {
            Touch::PostsHere => "posts here",
            Touch::LinkedSide => "linked company/person side",
            Touch::TransferTarget => "transfer target",
        }
    }
}

#[derive(Clone, Debug)]
pub struct AccountModel {
    pub id: AccountId,
    pub disclosure: Disclosure,
    pub ledger: ExplainedFigure,
    pub reserved: ExplainedFigure,
    pub free: ExplainedFigure,
    pub series: Vec<(SeriesId, Touch)>,
}

#[derive(Clone, Debug)]
pub struct EntityModels {
    pub persons: Vec<PersonModel>,
    pub companies: Vec<CompanyModel>,
    /// Only accounts whose existence the viewer may know.
    pub accounts: Vec<AccountModel>,
}

impl EntityModels {
    pub fn compute(household: &Household, viewer: Viewer) -> EngineResult<Self> {
        log::info!("computing entity models for viewer {}", viewer.person);
        let mut persons = Vec::new();
        let assessment = atlas_core::tax::assess(household, atlas_core::fixtures::default_horizon().max(household.as_of), None, atlas_core::forecast::Case::Expected).ok();
        for person in &household.people {
            let attribution = person_attribution(household, person.id)?;
            let tax_text = match &assessment {
                Some(assessment) => {
                    let own = assessment.by_entity.iter().find(|(e, _)| *e == EntityRef::Person(person.id)).map(|(_, c)| c.money());
                    let events = assessment.events.iter().filter(|e| e.entity == EntityRef::Person(person.id)).count();
                    match own {
                        Some(total) if events > 0 => format!("{} in the window from {events} tax event{} under the DEMO pack — a planning estimate, not a filing", total.format(), if events == 1 { "" } else { "s" }),
                        _ => "no tax event attributed to this person in the window".into(),
                    }
                }
                None => "tax assessment unavailable".into(),
            };
            persons.push(PersonModel {
                id: person.id,
                attribution: ExplainedFigure::new(
                    format!("person-{}-attribution", person.id.raw()),
                    "Attributed share of held accounts",
                    &attribution,
                    household,
                    viewer,
                ),
                income: household
                    .series
                    .iter()
                    .filter(|s| s.entity == EntityRef::Person(person.id) && s.direction == Direction::Income)
                    .map(|s| s.id)
                    .collect(),
                tax_text,
            });
        }

        let mut companies = Vec::new();
        for company in &household.companies {
            let disclosure = household.disclosure_for(viewer, ObjectRef::Company(company.id));
            if disclosure == Disclosure::Hidden {
                continue;
            }
            let cash = company_cash(household, company.id)?;
            let n = company.id.raw();
            companies.push(CompanyModel {
                id: company.id,
                disclosure,
                cash: ExplainedFigure::new(format!("company-{n}-cash"), "Business cash", &cash.cash, household, viewer),
                committed: ExplainedFigure::new(format!("company-{n}-committed"), "Committed obligations", &cash.committed, household, viewer),
                ceiling: ExplainedFigure::new(format!("company-{n}-ceiling"), "Cash-constraint ceiling before extraction costs", &cash.ceiling, household, viewer),
                extractable: ExplainedFigure::new(format!("company-{n}-extractable"), "Lawfully extractable cash", &cash.extractable, household, viewer),
            });
        }

        let mut accounts = Vec::new();
        for account in &household.accounts {
            let disclosure = household.disclosure_for(viewer, ObjectRef::Account(account.id));
            if matches!(disclosure, Disclosure::Hidden | Disclosure::Aggregate) {
                continue;
            }
            let liquidity = account_liquidity(household, account.id)?;
            let n = account.id.raw();
            let series = household
                .series
                .iter()
                .filter_map(|s| {
                    if s.account == account.id {
                        Some((s.id, Touch::PostsHere))
                    } else if s.linked_account == Some(account.id) {
                        Some((s.id, Touch::LinkedSide))
                    } else if matches!(s.direction, Direction::Transfer { to } if to == account.id) {
                        Some((s.id, Touch::TransferTarget))
                    } else {
                        None
                    }
                })
                .collect();
            accounts.push(AccountModel {
                id: account.id,
                disclosure,
                ledger: ExplainedFigure::new(format!("account-{n}-ledger"), "Settled ledger cash", &liquidity.ledger_cash, household, viewer),
                reserved: ExplainedFigure::new(format!("account-{n}-reserved"), "Reserved cash", &liquidity.reserved, household, viewer),
                free: ExplainedFigure::new(format!("account-{n}-free"), "Free current cash", &liquidity.free, household, viewer),
                series,
            });
        }

        Ok(EntityModels { persons, companies, accounts })
    }

    pub fn account(&self, id: AccountId) -> Option<&AccountModel> {
        self.accounts.iter().find(|a| a.id == id)
    }

    pub fn person(&self, id: PersonId) -> Option<&PersonModel> {
        self.persons.iter().find(|p| p.id == id)
    }

    pub fn company(&self, id: CompanyId) -> Option<&CompanyModel> {
        self.companies.iter().find(|c| c.id == id)
    }
}
