/* E03 is a deliberately small, one-account fixture, not the production engine.
 * All native amounts are integer currency minor units (100 minor = 1 unit).
 * Independent box bounds + additive cash + one uncertain inflow make the
 * low-income / high-expense / latest-inflow path pointwise worst for this fixture.
 * This shortcut must not be generalized to taxes, loans or contingent rules.
 */
(function (root) {
  'use strict';
  const U = 100;
  const START = '2026-09-10', END = '2027-01-31';
  function minor(value) {
    if (!Number.isFinite(value)) throw new TypeError('Amount must be finite.');
    const result = Math.round(value * U);
    if (!Number.isSafeInteger(result)) throw new RangeError('Amount exceeds exact integer range.');
    return result;
  }
  function events(path, purchaseDate, downPayment) {
    if (!['2026-11-15','2026-11-30'].includes(purchaseDate)) throw new RangeError('Unsupported fixture purchase date.');
    if (!['adverse','favorable'].includes(path)) throw new RangeError('Unknown bound.');
    if (!Number.isFinite(downPayment) || downPayment < 0) throw new RangeError('Down payment must be non-negative.');
    const adverse = path === 'adverse';
    const entries = [];
    function add(date,label,amount,order=10) { entries.push({date,label,amount:minor(amount),order}); }
    ['2026-09-30','2026-10-31','2026-11-30'].forEach(d=>add(d,'Salary receipt',adverse?480000:520000));
    ['2026-10-01','2026-11-01','2026-12-01','2027-01-01'].forEach(d=>add(d,'Rent',adverse?-200000:-180000));
    ['2026-10-15','2026-11-15','2026-12-15','2027-01-15'].forEach(d=>add(d,'Other spending',adverse?-140000:-120000));
    add(adverse?'2026-12-10':'2026-11-10','Client receipt',adverse?300000:400000);
    add(purchaseDate,'Down payment',-downPayment,20);
    entries.sort((a,b)=>a.date.localeCompare(b.date)||a.order-b.order||a.label.localeCompare(b.label));
    let balance=minor(2000000);
    let lowest=balance, lowestDate=START, firstBreach=null;
    const reserve=minor(1000000);
    const points=[{date:START,label:'Opening settled cash',balance,amount:0}];
    for(const e of entries){
      balance+=e.amount;
      if(!Number.isSafeInteger(balance)) throw new RangeError('Balance exceeds exact integer range.');
      if(balance<lowest){lowest=balance;lowestDate=e.date;}
      if(balance<reserve && !firstBreach)firstBreach=e.date;
      points.push({...e,balance});
    }
    points.push({date:END,label:'Horizon ends',balance,amount:0});
    return {points,lowest,lowestDate,firstBreach,ending:balance,reserve,headroom:lowest-reserve,starting:minor(2000000)};
  }
  function run({purchaseDate='2026-11-30',downPayment=1300000}={}) {
    return {adverse:events('adverse',purchaseDate,downPayment),favorable:events('favorable',purchaseDate,downPayment),purchaseDate,downPayment:minor(downPayment),start:START,end:END};
  }
  function valueAt(points,date){let value=points[0].balance;for(const p of points){if(p.date<=date)value=p.balance;else break;}return value;}
  const api=Object.freeze({run,events,valueAt,minor,U,START,END});
  root.DemoEngine=api;
  if(typeof module!=='undefined' && module.exports)module.exports=api;
})(typeof window!=='undefined'?window:globalThis);
