'use strict';

const inspector = require('inspector');

class Session extends inspector.Session {
  post(method, params) {
    return new Promise((resolve, reject) => {
      super.post(method, params, (error, result) => {
        if (error) reject(error);
        else resolve(result);
      });
    });
  }
}

module.exports = {
  open: inspector.open,
  close: inspector.close,
  url: inspector.url,
  waitForDebugger: inspector.waitForDebugger,
  Session,
};
