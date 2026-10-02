RETURN {
  LET $receipt = SELECT operation_id, request_id FROM ONLY $request;
  IF $receipt = NONE OR $receipt.operation_id != $operation_id OR $receipt.request_id != $request_id {
    THROW "computer_receipt_invalid";
  };
  SELECT * FROM ONLY $request WHERE operation_id = $operation_id AND request_id = $request_id;
};
