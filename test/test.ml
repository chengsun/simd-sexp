open! Core

let%expect_test _ =
  let print_io input =
    let input_length = String.length input in
    let indices =
      Bigarray.Array1.create Bigarray.int32 Bigarray.c_layout (String.length input)
    in
    let extract_structural_indices = Simd_sexp.Extract_structural_indices.create () in
    let input_index, indices_len =
      Simd_sexp.Extract_structural_indices.run
        extract_structural_indices
        ~input
        ~input_index:0
        ~indices
        ~indices_index:0
    in
    assert (input_index = String.length input);
    let n_set = Int.Hash_set.create () in
    for i = 0 to indices_len - 1 do
      Hash_set.add n_set (Int32.to_int_exn indices.{i})
    done;
    printf "%s\n" input;
    for i = 0 to input_length - 1 do
      if Hash_set.mem n_set i then printf "^" else printf "."
    done;
    printf "\n";
    (* Stage 2: check the state machine *)
    let state = Simd_sexp.State.create () in
    Simd_sexp.State.process_all state ~input
    |> List.iter ~f:(fun sexp -> printf !"> %{Sexp#hum}\n" sexp);
    printf "\n\n"
  in
  print_io {|foo|};
  print_io {|foo bar|};
  print_io {|foo   bar|};
  [%expect
    {|
    foo
    ^..
    > foo


    foo bar
    ^..^^..
    > foo
    > bar


    foo   bar
    ^..^..^..
    > foo
    > bar |}];
  print_io {|(foo   bar)|};
  print_io {|(fo\o   bar)|};
  print_io {|"()"|};
  print_io {|" "|};
  print_io {|"fo\"o"|};
  print_io {|fo\"o"|};
  [%expect
    {|
    (foo   bar)
    ^^..^..^..^
    > (foo bar)


    (fo\o   bar)
    ^^...^..^..^
    > ("fo\\o" bar)


    "()"
    ^..^
    > "()"


    " "
    ^.^
    > " "


    "fo\"o"
    ^.....^
    > "fo\"o"


    fo\"o"
    ^..^.^
    > "fo\\"
    > o |}];
  print_io {|(foo"x"bar)|};
  print_io {|(foo(x)bar)|};
  print_io {|("x"foo"y")|};
  print_io {|((x)foo(y))|};
  [%expect
    {|
    (foo"x"bar)
    ^^..^.^^..^
    > (foo x bar)


    (foo(x)bar)
    ^^..^^^^..^
    > (foo (x) bar)


    ("x"foo"y")
    ^^.^^..^.^^
    > (x foo y)


    ((x)foo(y))
    ^^^^^..^^^^
    > ((x) foo (y)) |}];
  print_io {|"foo\n"|};
  [%expect {|
    "foo\n"
    ^.....^
    > "foo\n" |}];
  print_io {|(foo "bar \"x\" baz" quux      )(foo "bar \"x\" baz" quux      )|};
  [%expect
    {|
    (foo "bar \"x\" baz" quux      )(foo "bar \"x\" baz" quux      )
    ^^..^^.............^.^...^.....^^^..^^.............^.^...^.....^
    > (foo "bar \"x\" baz" quux)
    > (foo "bar \"x\" baz" quux) |}];
  print_io
    {|(foo "bar \"x\" baz" quux      )(foo "bar \"x\" baz" quux      )(foo "bar \"x\" baz" quux      )(foo "bar \"x\" baz" quux      )|};
  [%expect
    {|
    (foo "bar \"x\" baz" quux      )(foo "bar \"x\" baz" quux      )(foo "bar \"x\" baz" quux      )(foo "bar \"x\" baz" quux      )
    ^^..^^.............^.^...^.....^^^..^^.............^.^...^.....^^^..^^.............^.^...^.....^^^..^^.............^.^...^.....^
    > (foo "bar \"x\" baz" quux)
    > (foo "bar \"x\" baz" quux)
    > (foo "bar \"x\" baz" quux)
    > (foo "bar \"x\" baz" quux) |}];
  print_io
    {| (foo "bar \"x\" baz" quux      )(foo "bar \"x\" baz" quux      )(foo "bar \"x\" baz" quux      )|};
  [%expect
    {|
   (foo "bar \"x\" baz" quux      )(foo "bar \"x\" baz" quux      )(foo "bar \"x\" baz" quux      )
  .^^..^^.............^.^...^.....^^^..^^.............^.^...^.....^^^..^^.............^.^...^.....^
  > (foo "bar \"x\" baz" quux)
  > (foo "bar \"x\" baz" quux)
  > (foo "bar \"x\" baz" quux) |}];
  print_io
    {|xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy|};
  [%expect
    {|
  xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy
  ^................................................................................................
  > xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy |}];
  print_io
    {|xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx  yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy|};
  [%expect
    {|
  xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx  yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy
  ^..............................................................^.^...............................
  > xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
  > yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy |}];
  print_io {|(                                                              "ab")|};
  [%expect
    {|
  (                                                              "ab")
  ^..............................................................^..^^
  > (ab) |}]
;;

(* TODO: comments *)
(* TODO: octal *)
