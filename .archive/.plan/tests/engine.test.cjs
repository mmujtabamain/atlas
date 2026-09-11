const {test}=require('node:test');
const assert=require('node:assert/strict');
const engine=require('../src/demo-engine.js');
const fixtures=[
 ['2026-11-15',1200000,1080000,'2026-11-15'],
 ['2026-11-15',1300000,980000,'2026-11-15'],
 ['2026-11-15',1400000,880000,'2026-11-15'],
 ['2026-11-30',1200000,1180000,'2027-01-15'],
 ['2026-11-30',1300000,1080000,'2027-01-15'],
 ['2026-11-30',1400000,980000,'2027-01-15']
];
for(const [purchaseDate,downPayment,lowest,lowestDate] of fixtures){
 test(`E03 ${downPayment} on ${purchaseDate}: original minimum and date`,()=>{
  const result=engine.run({purchaseDate,downPayment});
  assert.equal(result.adverse.lowest,engine.minor(lowest));
  assert.equal(result.adverse.lowestDate,lowestDate);
  assert.equal(result.adverse.starting,engine.minor(2000000));
 });
}
test('Future decisions do not change opening confirmed cash',()=>{
 for(let downPayment=800000;downPayment<=1600000;downPayment+=50000){
  const r=engine.run({downPayment});assert.equal(r.adverse.points[0].balance,200000000);
 }
});
test('Salary settles before the November 30 down payment',()=>{
 const points=engine.run().adverse.points.filter(p=>p.date==='2026-11-30');
 assert.deepEqual(points.map(p=>p.label),['Salary receipt','Down payment']);
});
test('No salary exists after November',()=>{
 const points=engine.run().adverse.points.filter(p=>p.label==='Salary receipt');
 assert.equal(points.length,3);assert.ok(points.every(p=>p.date<'2026-12-01'));
});
test('Every balance replays exactly in integer minor units',()=>{
 for(const date of ['2026-11-15','2026-11-30'])for(let downPayment=800000;downPayment<=1600000;downPayment+=50000){
  const r=engine.run({purchaseDate:date,downPayment});
  for(const p of [r.adverse,r.favorable]){let b=200000000;for(const event of p.points){b+=event.amount;assert.equal(event.balance,b);assert.ok(Number.isSafeInteger(b));}}
 }
});
test('Reserve is a constraint, not another cash payment',()=>{
 const r=engine.run().adverse;assert.equal(r.ending,engine.minor(2000000+3*480000+300000-4*200000-4*140000-1300000));
 assert.equal(r.headroom,r.lowest-r.reserve);
});
test('Favorable path never falls below the adverse path at end of event dates',()=>{
 const r=engine.run();const dates=[...new Set([...r.adverse.points,...r.favorable.points].map(p=>p.date))];
 dates.forEach(d=>assert.ok(engine.valueAt(r.favorable.points,d)>=engine.valueAt(r.adverse.points,d)));
});
test('Identical input is reproducible and prior runs do not mutate',()=>{
 const first=JSON.stringify(engine.run());engine.run({downPayment:1400000,purchaseDate:'2026-11-15'});
 assert.equal(JSON.stringify(engine.run()),first);
});
test('Fixture rejects unsupported dates, invalid numbers and overflow',()=>{
 assert.throws(()=>engine.run({purchaseDate:'2026-10-01'}),RangeError);
 assert.throws(()=>engine.run({downPayment:-1}),RangeError);
 assert.throws(()=>engine.run({downPayment:NaN}),RangeError);
 assert.throws(()=>engine.minor(Number.MAX_SAFE_INTEGER),RangeError);
});
