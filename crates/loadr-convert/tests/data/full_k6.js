import http from 'k6/http';
import { check, group, sleep } from 'k6';
import { Trend, Counter } from 'k6/metrics';
import { randomString } from 'https://jslib.k6.io/k6-utils/1.2.0/index.js';

const loginTime = new Trend('login_time');
const orders = new Counter('orders_placed');

export const options = {
  stages: [
    { duration: '30s', target: 10 },
    { duration: '1m', target: 10 },
  ],
};

export function setup() {
  const res = http.post('https://shop.example.com/auth', JSON.stringify({ user: 'admin' }));
  return { token: res.json('token') };
}

export default function (data) {
  group('storefront', () => {
    const home = http.get('https://shop.example.com/');
    check(home, {
      'home ok': (r) => r.status === 200 || r.status === 304,
    });
    sleep(2);
  });

  group('order', () => {
    const res = http.post('https://shop.example.com/orders', JSON.stringify({ sku: 'X1' }));
    check(res, {
      'created': (r) => r.status === 201,
      'has id': (r) => r.json('id') !== undefined,
    });
    orders.add(1);
  });
}
