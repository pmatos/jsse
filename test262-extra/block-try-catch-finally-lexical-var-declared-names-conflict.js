// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-block-static-semantics-early-errors
description: try, catch and finally blocks reject lexical declarations that collide with var-scoped names or each other
info: |
  Block : { StatementList }

  It is a Syntax Error if the LexicallyDeclaredNames of StatementList contains
  any duplicate entries.

  It is a Syntax Error if any element of the LexicallyDeclaredNames of
  StatementList also occurs in the VarDeclaredNames of StatementList.

  TryStatement : try Block Catch
  TryStatement : try Block Finally
  Catch : catch ( CatchParameter ) Block
  Finally : finally Block

  The Block of a try statement, the catch clause and the finally clause are
  all Block productions, so the Block early errors apply to each.
---*/

function bad(source) {
  assert.throws(SyntaxError, function () { Function(source); }, source);
}

bad("try { let a; var a; } catch (e) {}");
bad("try { const a = 1; { var a; } } catch (e) {}");
bad("try { let a; let a; } catch (e) {}");
bad("try { function a() {} var a; } catch (e) {}");
bad("try { } catch (e) { let a; var a; }");
bad("try { } catch { class a {} var a; }");
bad("try { } catch (e) { const a = 1; const a = 2; }");
bad("try { } finally { let a; var a; }");
bad("try { } finally { function a() {} let a; }");
bad("try { } catch (e) { let e; }");

Function("try { let a; { var b; } } catch (e) { var e; let a; } finally { const a = 1; }");
Function("try { function a() {} function a() {} } catch (e) {}");
Function("try { let a; { let a; } } catch (e) { let a; }");
