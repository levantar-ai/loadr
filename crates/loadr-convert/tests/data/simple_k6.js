import http from 'k6/http';
import { check, sleep } from 'k6';

export const options = {
  vus: 10,
  duration: '30s',
  thresholds: {
    http_req_duration: ['p(95)<400'],
    http_req_failed: ['rate<0.01'],
  },
};

export default function () {
  const res = http.get('https://test.example.com/api/users');
  check(res, {
    'status is 200': (r) => r.status === 200,
    'body has users': (r) => r.body.includes('users'),
    'fast enough': (r) => r.timings.duration < 500,
  });
  sleep(1);
}
