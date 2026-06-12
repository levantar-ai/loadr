import http from 'k6/http';
import { sleep } from 'k6';

export const options = {
  scenarios: {
    browse: {
      executor: 'ramping-vus',
      startVUs: 0,
      stages: [
        { duration: '1m', target: 20 },
        { duration: '3m', target: 20 },
        { duration: '30s', target: 0 },
      ],
      gracefulRampDown: '10s',
      exec: 'browse',
    },
    api: {
      executor: 'constant-arrival-rate',
      rate: 50,
      timeUnit: '1s',
      duration: '2m',
      preAllocatedVUs: 10,
      maxVUs: 50,
      startTime: '10s',
      exec: 'apiLoad',
    },
  },
  thresholds: {
    http_req_duration: [
      'p(95)<500',
      { threshold: 'p(99)<1500', abortOnFail: true, delayAbortEval: '30s' },
    ],
    checks: 'rate>0.95',
  },
};

export function browse() {
  http.get(`${__ENV.BASE_URL}/products`);
  sleep(Math.random() * 2 + 1);
}

export function apiLoad() {
  http.post('https://api.example.com/orders', JSON.stringify({ item: 42, qty: 1 }), {
    headers: { 'Content-Type': 'application/json' },
  });
}
