open! Core

let%expect_test "of_string -> to_string roundtrip" =
  let test string =
    string |> Simd_sexp.Tape.of_string |> Simd_sexp.Tape.to_string |> print_endline
  in
  test "a";
  [%expect {| a |}];
  test "ab";
  [%expect {| ab |}];
  test "abc";
  [%expect {| abc |}];
  test "abcd";
  [%expect {| abcd |}];
  test "abcde";
  [%expect {| abcde |}];
  test "abcdef";
  [%expect {| abcdef |}];
  test "abcdefg";
  [%expect {| abcdefg |}];
  test "abcdefgh";
  [%expect {| abcdefgh |}];
  test {|"foo"|};
  [%expect {| foo |}];
  test {|"foo bar"|};
  [%expect {| "foo bar" |}];
  test {|"foo()bar"|};
  [%expect {| "foo()bar" |}];
  test {|"a\x00"|};
  [%expect {| "a\000" |}];
  test {|"ab\x00"|};
  [%expect {| "ab\000" |}];
  test {|"abc\x00"|};
  [%expect {| "abc\000" |}];
  test {|"abcd\x00"|};
  [%expect {| "abcd\000" |}];
  test "()";
  [%expect {| () |}];
  test "(())";
  [%expect {| (()) |}];
  test "(a)";
  [%expect {| (a) |}];
  test "(a b)";
  [%expect {| (a b) |}];
  test "(a () b)";
  [%expect {| (a()b) |}];
  test "(() ())";
  [%expect {| (()()) |}];
  test "((a) ())";
  [%expect {| ((a)()) |}];
  test "((a b) (c d))";
  [%expect {| ((a b)(c d)) |}]
;;

let%expect_test "of_string errors" =
  let test string =
    Expect_test_helpers_base.require_does_raise [%here] (fun () ->
        string |> Simd_sexp.Tape.of_string)
  in
  test "";
  [%expect {| "expected nonempty list; got empty" |}];
  test "   ";
  [%expect {| "expected nonempty list; got empty" |}];
  test "a b";
  [%expect {| "expected empty list; got nonempty" |}];
  test "a ()";
  [%expect {| "expected empty list; got nonempty" |}];
  test "()()";
  [%expect {| "expected empty list; got nonempty" |}]
;;


let%expect_test "of_string_multi -> to_string_multi roundtrip" =
  let test string =
    string |> Simd_sexp.Tape.of_string_multi |> Simd_sexp.Tape.to_string_multi |> print_endline
  in
  test "a";
  [%expect {| a |}];
  test "a b";
  [%expect {| a b |}];
  test "a () b";
  [%expect {| a()b |}];
  test "";
  [%expect {| |}];
;;

let%expect_test "of_string_multi errors" =
  let test string =
    Expect_test_helpers_base.require_does_raise [%here] (fun () ->
        string |> Simd_sexp.Tape.of_string)
  in
  test "(";
  [%expect {| "Unmatched open paren" |}];
;;

let%expect_test "atom_to_string" =
  let test string =
    string
    |> Simd_sexp.Tape.of_string
    |> Simd_sexp.Tape.destruct_atom_exn
    |> Simd_sexp.Tape.atom_to_string
    |> print_endline
  in
  test "foo";
  [%expect {| foo |}];
  test {|"foo"|};
  [%expect {| foo |}];
  test {|"foo bar"|};
  [%expect {| foo bar |}];
  test {|"foo\nbar"|};
  [%expect {|
    foo
    bar |}];
  test {|"foo\010bar"|};
  [%expect {|
    foo
    bar |}];
;;

let%expect_test "parse_multi_partial" =
  let partial state string =
    string
    |> Simd_sexp.Tape.parse_multi_partial state
    |> ok_exn
    |> Simd_sexp.Tape.to_string_multi
    |> print_endline
  in
  let eof state =
    Simd_sexp.Tape.parse_multi_eof state
    |> ok_exn
    |> Simd_sexp.Tape.to_string_multi
    |> print_endline
  in
  let state = Simd_sexp.Tape.Parser_state.create () in
  partial state "";
  [%expect {| |}];
  partial state "x";
  [%expect {| |}];
  partial state "y";
  [%expect {| |}];
  partial state " ";
  [%expect {| xy |}];
  partial state "a";
  [%expect {| |}];
  partial state "a b (";
  [%expect {| aa b |}];
  partial state "c)";
  [%expect {| |}];
  eof state;
  [%expect {| (c) |}];
;;
