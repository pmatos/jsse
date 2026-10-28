// RUN: 1
// MODE: eval
// An arrow called by the timer is named after the timer: Timeout._onTimeout.
setTimeout(() => {
  throw new Error('late');
}, 1);
