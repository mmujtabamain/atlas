/* Editorial navigation and diagrams. Full requirements remain in data/atlas.json.
 * Connections are conceptual dependencies, NOT delivery order or a final schema.
 */
window.ATLAS_EDITORIAL = {
 groups: [
  ['Financial foundations','wallet','teal','Know what is owned, settled, reserved and actually accessible.'],
  ['Uncertainty & risk','branch','violet','Describe possible futures without inventing confidence.'],
  ['Decisions & goals','target','blue','Compare alternatives, competing goals and conditional actions.'],
  ['Taxes & extraction','coin','amber','Model lawful, after-tax funding across accounts, entities and years.'],
  ['Banks & funding routes','bank','teal','Respect transfer paths, fees, cutoffs, caps and deposit maturity.'],
  ['Debt & contracts','document','blue','Represent the actual loan or card contract, not just a monthly payment.'],
  ['Purchases & investments','car','violet','Compare complete ownership costs, purchasing power and asset risk.'],
  ['Companies & payroll','building','amber','Keep companies distinct while linking their real effects on the household.'],
  ['Insurance & lifetime','heart','rose','Plan for retirement, retained losses, dependents and financial shocks.'],
  ['Shared finance & data','people','blue','Coordinate contributions and preserve the quality of financial assumptions.'],
  ['Calculation integrity','test','teal','Replay results, disclose approximation and verify numerical behavior.'],
  ['Adaptable rules','rule','violet','Make financial rules editable, versioned, testable and explainable.'],
  ['Privacy & authorization','shield','teal','Separate ownership, data visibility, calculation permission and disclosure.']
 ],
 principles:[
  {icon:'wallet',title:'Future money ≠ current cash',text:'Only settled, accessible funds count as available now.',section:'s6'},
  {icon:'rule',title:'Deterministic. No AI.',text:'Explicit inputs, rules and methods drive every calculation.',section:'s2'},
  {icon:'building',title:'Company money stays distinct',text:'Ownership is not permission to spend company cash.',section:'s8'},
  {icon:'shield',title:'Explain, without exposing',text:'Complete provenance; only authorized disclosure.',section:'M55'}
 ],
 questions:[
  {icon:'people',title:'What do we actually have?',text:'People, joint ownership, current balances, reserves and accessible money.',target:'g1',model:'M02',tone:'teal'},
  {icon:'branch',title:'What could our future look like?',text:'Ending salaries, delayed invoices, changing expenses and connected shocks.',target:'g2',model:'M08',tone:'violet'},
  {icon:'car',title:'When could we make a big purchase?',text:'Dates, down payments, financing, safety floors and competing goals.',target:'g3',model:'M19',tone:'blue'},
  {icon:'coin',title:'How should we fund it?',text:'Lawful extraction, taxes, timing, fees and account-level constraints.',target:'g4',model:'M26',tone:'amber'},
  {icon:'building',title:'How do our companies fit in?',text:'Payroll, business obligations, ownership and authorized household flows.',target:'g8',model:'M27',tone:'amber'},
  {icon:'shield',title:'Can we plan together, privately?',text:'Purpose-specific contributions and permission-aware explanations.',target:'g13',model:'M55',tone:'teal'}
 ],
 maps: {
  entities:{
   title:'The domain, connected',description:'Select any object to see its relationships and the requirements behind it.',
   columns:['People & entities','Financial records','Planning context','Trust & outputs'],
   nodes:[
    {id:'household',x:25,y:68,label:'Household',sub:'Planning boundary',icon:'people',ref:'s5',text:'A shared planning group. Aggregation must not erase legal ownership, third-party interests or company boundaries.'},
    {id:'people',x:25,y:232,label:'People',sub:'Owners & participants',icon:'people',ref:'s7',text:'A person can own several accounts and companies. Economic ownership, visibility and contribution responsibility are separate facts.'},
    {id:'companies',x:25,y:396,label:'Companies',sub:'Separate legal entities',icon:'building',ref:'s8',text:'Companies retain independent ledgers, owners, permissions and obligations. Their bank balances do not become household spending money.'},
    {id:'accounts',x:275,y:68,label:'Accounts',sub:'Balances & settlement',icon:'bank',ref:'s6',text:'Money is held in specific accounts, currencies and legal boundaries. Reserves, transfer access and settlement dates determine usability.'},
    {id:'events',x:275,y:232,label:'Events & actuals',sub:'What happened / may happen',icon:'calendar',ref:'s9',text:'Scheduled events and actual transactions remain distinct and are reconciled without double-counting. Event series can start, stop and change.'},
    {id:'payroll',x:275,y:396,label:'Payroll & owner flows',sub:'Linked economic movements',icon:'coin',ref:'M41',text:'Gross pay, net receipts and remittances are connected movements across distinct ledgers. Each side has its own authorized representation.'},
    {id:'assumptions',x:525,y:68,label:'Assumptions',sub:'Amounts, dates & dependencies',icon:'branch',ref:'s10',text:'User-approved assumptions describe uncertain future paths. Date ranges and shared causes matter as much as amounts.'},
    {id:'scenarios',x:525,y:232,label:'Scenarios & goals',sub:'Alternative futures',icon:'target',ref:'s18',text:'Scenario overlays change future events, funding choices or goals without rewriting current financial reality. They have independent privacy boundaries.'},
    {id:'rules',x:525,y:396,label:'Rules & taxes',sub:'Versioned constraints',icon:'rule',ref:'s14',text:'Effective-dated rules govern taxes, fees, transfers, funding eligibility and financial objectives. Conflicts and unsupported properties must be explicit.'},
    {id:'policies',x:775,y:68,label:'Access & disclosure',sub:'Purpose-specific authority',icon:'shield',ref:'M55',text:'Calculation access is not raw visibility, and neither is permission to disclose a result. Derived totals must not reconstruct restricted inputs.'},
    {id:'results',x:775,y:232,label:'Decision results',sub:'Conditional, not promises',icon:'chart',ref:'s26',text:'Results name their assumptions, scope, horizon, constraints and strength of evidence. Recommendations do not authorize financial execution.'},
    {id:'provenance',x:775,y:396,label:'Provenance & replay',sub:'Why this result?',icon:'test',ref:'s24',text:'The complete calculation graph records state, assumptions, rule versions and methods. Every viewer receives only an authorized projection of that graph.'}
   ],
   edges:[
    ['household','people','Contains participants; does not redefine legal ownership'],
    ['people','companies','Owns or operates, subject to ownership shares'],
    ['people','accounts','Economic ownership is explicitly recorded'],
    ['companies','payroll','Employs people and authorizes lawful owner flows'],
    ['companies','accounts','Owns separate company accounts; ownership does not make the cash personally spendable'],
    ['accounts','events','Actual movements update balances; planned events forecast them'],
    ['payroll','events','Links company and recipient records once'],
    ['events','scenarios','Provides dated baseline movements'],
    ['assumptions','scenarios','Supplies explicit uncertain amounts, dates and dependencies'],
    ['rules','scenarios','Constrains and transforms each permitted future'],
    ['payroll','rules','Requires taxes, remittances and legal capacity'],
    ['scenarios','results','Produces compared, assumption-bound outcomes'],
    ['policies','results','Controls purpose, use and permitted disclosure'],
    ['accounts','policies','Carries object-level access policies'],
    ['results','provenance','Retains a complete calculation graph and replay record']
   ]
  },
  flow:{
   title:'How a result is produced',description:'A calculation pipeline, not an implementation sequence. Trust boundaries apply throughout.',
   columns:['Inputs','Model context','Compute & verify','Authorize disclosure'],
   nodes:[
    {id:'state',x:25,y:125,label:'Reconciled state',sub:'Current truth',icon:'wallet',ref:'M01',text:'Start from confirmed, currency-specific, dated financial records. Future income cannot increase today’s available cash.'},
    {id:'future',x:25,y:335,label:'Future events',sub:'User-approved assumptions',icon:'calendar',ref:'M07',text:'Build coherent paths with recurrence, employment changes, uncertain dates and shared shocks.'},
    {id:'policy',x:275,y:125,label:'Authorization context',sub:'Viewer, purpose & time',icon:'shield',ref:'M55',text:'Resolve which data may participate and which outputs may be disclosed. Missing or conflicting grants fail closed.'},
    {id:'rules',x:275,y:335,label:'Versioned model',sub:'Taxes, contracts & objectives',icon:'rule',ref:'M54',text:'Evaluate typed rules with declared mathematical properties. Match the solution method to the model actually specified.'},
    {id:'engine',x:525,y:125,label:'Evaluate alternatives',sub:'Paths, constraints & choices',icon:'math',ref:'M51',text:'Apply deterministic calculation or optimization methods. Conditional policies cannot act on information that has not been revealed.'},
    {id:'replay',x:525,y:335,label:'Independent replay',sub:'Minor-unit validation',icon:'test',ref:'M53',text:'Recompute proposed actions against dates, actual contract rules and decimal-level rounding. Preserve method and tolerance evidence.'},
    {id:'result',x:775,y:125,label:'Authorized result',sub:'Claim + assumptions + limits',icon:'chart',ref:'s26',text:'Give the user alternatives and their trade-offs, not a hidden score or unconditional assurance. No action is executed.'},
    {id:'audit',x:775,y:335,label:'Inspectable evidence',sub:'Permission-aware provenance',icon:'document',ref:'s24',text:'Keep complete provenance internally while suppressing, coarsening or blocking outputs that would expose restricted information.'}
   ],
   edges:[['state','policy','Limits usable input state'],['future','rules','Supplies explicit path assumptions'],['policy','engine','Authorizes use for this purpose'],['rules','engine','Defines valid calculations and decisions'],['engine','replay','Submits a candidate plus method status'],['replay','result','Validates replay before a claim is published'],['result','audit','Exposes permitted calculation evidence'],['policy','result','Separately authorizes output disclosure']]
  }
 },
 journeys:[
  {title:'A person leaves a job',icon:'calendar',text:'End the salary series, model final pay and delayed receipts, then inspect reserve breaches and goal impacts.',links:['F013','F021','F025','M05'],sections:['s9','s10']},
  {title:'A company pays its owner',icon:'building',text:'Protect payroll and working capital, validate the extraction route, calculate both sides’ taxes and link the net household receipt.',links:['F045','F046','F088','F093'],sections:['M27','M41']},
  {title:'Choose an account to withdraw from',icon:'bank',text:'Compare lawful net proceeds after tax, fees, transfer timing and thresholds. Keep future tax and debt obligations in the horizon.',links:['F041','F043','F044','F049'],sections:['M25','M26']},
  {title:'A client pays late',icon:'clock',text:'Shift settlement rather than pretending the money has arrived. Test whether salaries, bills or purchase commitments need to change.',links:['F013','F027','F090'],sections:['M13','M43']},
  {title:'Private funds support a shared goal',icon:'shield',text:'Authorize a bounded contribution for one purpose, protect the underlying account, and validate that the result cannot leak restricted details.',links:['F147','F151','F154','F161'],sections:['M55']},
  {title:'A large purchase competes with a goal',icon:'target',text:'Compare dates, financing and full ownership costs while exposing trade-offs against the other goal and the required cash floor.',links:['F030','F034','F075','F076'],sections:['M19','M36']}
 ]
};
