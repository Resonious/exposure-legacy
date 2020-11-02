
module Exposure
  module DefineMethodMutator
    def define_method(*)
      super
    end
  end
end

Class.prepend Exposure::DefineMethodMutator
